/**
 * Settings → Source trust (source provenance §13, §19): the user's site
 * overrides with what the claims under them did since, the bars in force for
 * every claim kind and level with each automatic change and its numbers, the
 * policy parameters with their derivations, the checker's measured
 * reliability, and the provenance eval export.
 */

import { Check, Minus } from 'lucide-react';

import { PageHeader, SettingsSection } from '@/components/ui';
import { accuracyLine } from '@/features/learning/lessons/review/CheckerAccuracy';
import { kindWord, LEVEL_WORD, overrideLines } from '@/features/learning/sources/trust/trustPresentation';
import {
  useCheckerReliability,
  useClearSourceTrustOverride,
  useExportProvenanceEval,
  useSourceTrustCalibration,
  useSourceTrustOverrides,
} from '@/features/learning/sources/trust/useSourceTrust';
import type { LearningLevelBarDto, LearningSiteTrustOverrideDto, LearningTrustOverrideScopeDto } from '@/lib/bindings';

const percent = (value: number) => `${Math.round(value * 100)}%`;

function day(seconds: number | null) {
  if (seconds == null) return null;
  const date = new Date(seconds * 1000);
  return Number.isNaN(date.getTime()) ? null : date.toISOString().slice(0, 10);
}

function scopeOf(entry: LearningSiteTrustOverrideDto['trustOverride']): LearningTrustOverrideScopeDto | null {
  if (entry.scope === 'site') return { scope: 'site', registrableDomain: entry.registrableDomain };
  if (entry.scope === 'site_subject') return { scope: 'site_subject', registrableDomain: entry.registrableDomain, class: entry.class };
  return null;
}

/** "2 of 9 claims its pages supported were overturned". */
function recordLine(record: { right: number; wrong: number }) {
  const total = record.right + record.wrong;
  if (total === 0) return 'No verdicts on its pages yet.';
  return `${record.wrong} of ${total} ${total === 1 ? 'claim' : 'claims'} its pages supported ${record.wrong === 1 ? 'was' : 'were'} overturned.`;
}

/** One level under one kind's bar: accepted alone or not, its verdicts, and any move. */
function BarCell({ bar }: { bar: LearningLevelBarDto }) {
  const verdicts = bar.held + bar.failed;
  const since = day(bar.since);
  return (
    <td className="border-t border-border px-2 py-1.5 align-top">
      <span className="inline-flex items-center gap-1 font-medium text-text-primary">
        {bar.acceptedAlone ? <Check size={12} aria-hidden="true" /> : <Minus size={12} aria-hidden="true" />}
        {bar.acceptedAlone ? 'Alone' : 'Not alone'}
      </span>
      <span className="block text-text-muted">{verdicts ? `${verdicts} ${verdicts === 1 ? 'verdict' : 'verdicts'}, ${percent(bar.held / verdicts)} held` : 'No verdicts'}</span>
      {bar.basis !== 'static' && <span className="block text-text-secondary">{bar.basis === 'relaxed' ? 'Relaxed' : 'Tightened'}{since ? ` since ${since}` : ''}</span>}
    </td>
  );
}

export function SourceTrustSettings() {
  const overrides = useSourceTrustOverrides();
  const calibration = useSourceTrustCalibration();
  const checker = useCheckerReliability();
  const clear = useClearSourceTrustOverride();
  const exported = useExportProvenanceEval();
  const levels = calibration.data?.kinds[0]?.levels.map((bar) => bar.level) ?? [];
  const changes = calibration.data?.changes ?? [];

  return (
    <>
      <PageHeader title="Source trust" meta="How Learning Studio weighs who published what a lesson cites." />

      <SettingsSection title="Your site overrides" description="A site override says who the publisher is; only a site trusted for a subject can be primary for it.">
        {overrides.isLoading && <p role="status" className="text-sm text-text-muted">Loading…</p>}
        {overrides.error && <p role="alert" className="text-sm text-rose-700">{overrides.error.message}</p>}
        {overrides.data?.length === 0 && <p className="text-sm text-text-secondary">None yet. Set one from a source’s “Change trust…” menu.</p>}
        {overrides.data && overrides.data.length > 0 && (
          <ul className="space-y-2">
            {overrides.data.map((entry) => {
              const line = overrideLines([entry.trustOverride])[0];
              const scope = scopeOf(entry.trustOverride);
              return (
                <li key={line} className="flex flex-wrap items-start justify-between gap-2 rounded-lg border border-border p-3 text-sm">
                  <div className="min-w-0">
                    <p className="font-medium text-text-primary">{line}</p>
                    {entry.trustOverride.note && <p className="text-xs text-text-secondary">“{entry.trustOverride.note}”</p>}
                    <p className="text-xs text-text-muted">{recordLine(entry.trackRecord)}</p>
                  </div>
                  {scope && <button type="button" disabled={clear.isPending} onClick={() => clear.mutate(scope)} aria-label={`Reset: ${line}`} className="min-h-8 rounded-full border border-border px-3 text-xs font-medium text-text-primary disabled:opacity-50">Reset</button>}
                </li>
              );
            })}
          </ul>
        )}
        {clear.error && <p role="alert" className="text-sm text-rose-700">{clear.error.message}</p>}
      </SettingsSection>

      <SettingsSection title="Bars in force" description="Which level of source is enough alone for each kind of claim. Your verdicts move a bar in either direction once the evidence is strong enough.">
        {calibration.isLoading && <p role="status" className="text-sm text-text-muted">Loading…</p>}
        {calibration.error && <p role="alert" className="text-sm text-rose-700">{calibration.error.message}</p>}
        {calibration.data && (
          <>
            <div className="overflow-x-auto">
              <table className="w-full min-w-[36rem] border-collapse text-left text-xs text-text-secondary">
                <caption className="pb-2 text-left text-text-muted">
                  {calibration.data.verdicts} {calibration.data.verdicts === 1 ? 'verdict' : 'verdicts'} recorded · policy {calibration.data.policyVersion}
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="px-2 py-1.5 font-medium text-text-muted">Claim kind</th>
                    {levels.map((level) => <th key={level} scope="col" className="px-2 py-1.5 font-medium text-text-muted">{LEVEL_WORD[level]}</th>)}
                  </tr>
                </thead>
                <tbody>
                  {calibration.data.kinds.map((kind) => (
                    <tr key={kind.claimKind}>
                      <th scope="row" className="border-t border-border px-2 py-1.5 align-top font-medium text-text-primary">
                        {kindWord(kind.claimKind)}
                        <span className="block font-normal text-text-muted">{kind.lowestAccepted ? `Alone from ${LEVEL_WORD[kind.lowestAccepted].toLowerCase()}` : 'Only corroboration'}</span>
                      </th>
                      {kind.levels.map((bar) => <BarCell key={bar.level} bar={bar} />)}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <h3 className="mt-4 text-sm font-medium text-text-primary">Automatic changes</h3>
            {changes.length === 0
              ? <p className="text-sm text-text-secondary">None: every bar is the design’s table.</p>
              : <ul className="mt-1 list-disc space-y-1 pl-5 text-sm text-text-secondary">{changes.map((change) => <li key={`${change.claimKind}-${change.level}-${change.at}-${change.claimId}`}>{change.reason.text}</li>)}</ul>}
          </>
        )}
      </SettingsSection>

      {calibration.data && (
        <SettingsSection title="Policy" description="The stated parameters these rules compare against, and where each comes from.">
          <dl className="space-y-3 text-sm">
            {calibration.data.parameters.map((parameter) => (
              <div key={parameter.name}>
                <dt className="font-mono text-xs text-text-primary">{parameter.name} = {parameter.value}</dt>
                <dd className="text-text-secondary">{parameter.derivation}</dd>
              </div>
            ))}
          </dl>
        </SettingsSection>
      )}

      <SettingsSection title="Checker reliability" description="How often the local checker was wrong on claims you labelled, per checker model.">
        {checker.isLoading && <p role="status" className="text-sm text-text-muted">Loading…</p>}
        {checker.error && <p role="alert" className="text-sm text-rose-700">{checker.error.message}</p>}
        {checker.data?.length === 0 && <p className="text-sm text-text-secondary">No labelled claims yet. Each confirmation, removal, edit or added source labels one.</p>}
        {checker.data?.map((row) => <p key={row.checkerModel} className="text-sm text-text-secondary"><span className="font-medium text-text-primary">{row.checkerModel}: </span>{accuracyLine(row)}</p>)}
      </SettingsSection>

      <SettingsSection title="Evaluation" description="Your overrides as labelled examples for the provenance eval (evals/provenance).">
        <button type="button" disabled={exported.isPending} onClick={() => exported.mutate()} className="min-h-9 rounded-full border border-border px-4 text-sm font-medium text-text-primary disabled:opacity-50">
          {exported.isPending ? 'Exporting…' : 'Export provenance eval'}
        </button>
        {exported.data && <p role="status" className="mt-2 text-sm text-text-secondary">Saved {exported.data.examples} {exported.data.examples === 1 ? 'example' : 'examples'} to {exported.data.path}.</p>}
        {exported.isSuccess && exported.data === null && <p role="status" className="mt-2 text-sm text-text-secondary">Export cancelled.</p>}
        {exported.error && <p role="alert" className="mt-2 text-sm text-rose-700">{exported.error.message}</p>}
      </SettingsSection>
    </>
  );
}
