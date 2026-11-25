/**
 * Settings → Source trust (source provenance §13, §19): the user's site
 * overrides with what the claims under them did since, the bars in force for
 * every claim kind and level with each automatic change and its numbers, the
 * policy parameters with their derivations, the checker's measured
 * reliability, and the provenance eval export.
 */

import { Check, Minus } from 'lucide-react';

import { Button, PageHeader, SettingsSection } from '@/components/ui';
import { accuracyLine } from '@/features/learning/lessons/review/CheckerAccuracy';
import {
  kindWord,
  LEVEL_WORD,
  overrideLines,
} from '@/features/learning/sources/trust/trustPresentation';
import {
  useCheckerReliability,
  useClearSourceTrustOverride,
  useExportProvenanceEval,
  useSourceTrustCalibration,
  useSourceTrustOverrides,
} from '@/features/learning/sources/trust/useSourceTrust';
import type {
  LearningLevelBarDto,
  LearningSiteTrustOverrideDto,
  LearningTrustOverrideScopeDto,
} from '@/lib/bindings';

const percent = (value: number) => `${Math.round(value * 100)}%`;

function day(seconds: number | null) {
  if (seconds == null) return null;
  const date = new Date(seconds * 1000);
  return Number.isNaN(date.getTime()) ? null : date.toISOString().slice(0, 10);
}

function scopeOf(
  entry: LearningSiteTrustOverrideDto['trustOverride'],
): LearningTrustOverrideScopeDto | null {
  if (entry.scope === 'site') return { scope: 'site', registrableDomain: entry.registrableDomain };
  if (entry.scope === 'site_subject') {
    return {
      scope: 'site_subject',
      registrableDomain: entry.registrableDomain,
      class: entry.class,
    };
  }
  return null;
}

/** "2 of 9 claims its pages supported were overturned". */
function recordLine(record: { right: number; wrong: number }) {
  const total = record.right + record.wrong;
  if (total === 0) return 'No verdicts on its pages yet.';
  return `${record.wrong} of ${total} ${total === 1 ? 'statement' : 'statements'} its pages supported ${record.wrong === 1 ? 'was' : 'were'} overturned.`;
}

/** One level under one kind's bar: accepted alone or not, its verdicts, and any move. */
function BarCell({ bar }: { bar: LearningLevelBarDto }) {
  const verdicts = bar.held + bar.failed;
  const since = day(bar.since);
  return (
    <td className="border-t border-border-subtle px-2 py-2 align-top">
      <span className="inline-flex items-center gap-1 font-medium text-text-primary">
        {bar.acceptedAlone ? (
          <Check size={12} aria-hidden="true" />
        ) : (
          <Minus size={12} aria-hidden="true" />
        )}
        {bar.acceptedAlone ? 'Alone' : 'Not alone'}
      </span>
      <span className="block tabular-nums text-text-muted">
        {verdicts
          ? `${verdicts} ${verdicts === 1 ? 'verdict' : 'verdicts'}, ${percent(bar.held / verdicts)} held`
          : 'No verdicts'}
      </span>
      {bar.basis !== 'static' && (
        <span className="block text-text-secondary">
          {bar.basis === 'relaxed' ? 'Relaxed' : 'Tightened'}
          {since ? ` since ${since}` : ''}
        </span>
      )}
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
      <PageHeader
        title="Source trust"
        meta="How Learning Studio weighs who published what a lesson cites."
      />

      <SettingsSection
        title="Your site overrides"
        description="A site override says who the publisher is; only a site trusted for a subject can be primary for it."
      >
        {overrides.isLoading && (
          <p role="status" className="py-3 text-sm text-text-muted">
            Loading…
          </p>
        )}
        {overrides.error && (
          <p role="alert" className="py-3 text-sm text-danger-fg">
            {overrides.error.message}
          </p>
        )}
        {overrides.data?.length === 0 && (
          <p className="py-3 text-sm text-text-secondary">
            None yet. Set one from a source’s “Change trust…” menu.
          </p>
        )}
        {overrides.data && overrides.data.length > 0 && (
          <ul className="divide-y divide-border-subtle">
            {overrides.data.map((entry) => {
              const line = overrideLines([entry.trustOverride])[0];
              const scope = scopeOf(entry.trustOverride);
              return (
                <li
                  key={line}
                  className="flex flex-wrap items-start justify-between gap-3 py-3 text-sm"
                >
                  <div className="min-w-0">
                    <p className="font-medium text-text-primary">{line}</p>
                    {entry.trustOverride.note && (
                      <p className="mt-0.5 text-xs text-text-secondary">
                        “{entry.trustOverride.note}”
                      </p>
                    )}
                    <p className="mt-0.5 text-xs text-text-muted">
                      {recordLine(entry.trackRecord)}
                    </p>
                  </div>
                  {scope && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={clear.isPending}
                      onClick={() => clear.mutate(scope)}
                      aria-label={`Reset: ${line}`}
                    >
                      Reset
                    </Button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
        {clear.error && (
          <p role="alert" className="py-2 text-sm text-danger-fg">
            {clear.error.message}
          </p>
        )}
      </SettingsSection>

      <SettingsSection
        title="Bars in force"
        description="Which level of source is enough alone for each kind of statement. Your verdicts move a bar in either direction once the evidence is strong enough."
      >
        {calibration.isLoading && (
          <p role="status" className="py-3 text-sm text-text-muted">
            Loading…
          </p>
        )}
        {calibration.error && (
          <p role="alert" className="py-3 text-sm text-danger-fg">
            {calibration.error.message}
          </p>
        )}
        {calibration.data && (
          <div className="py-3">
            <div className="overflow-x-auto">
              <table className="w-full min-w-[36rem] border-collapse text-left text-xs text-text-secondary">
                <caption className="pb-2 text-left tabular-nums text-text-muted">
                  {calibration.data.verdicts}{' '}
                  {calibration.data.verdicts === 1 ? 'verdict' : 'verdicts'} recorded · policy{' '}
                  {calibration.data.policyVersion}
                </caption>
                <thead>
                  <tr>
                    <th scope="col" className="px-2 py-1.5 font-medium text-text-muted">
                      Statement kind
                    </th>
                    {levels.map((level) => (
                      <th key={level} scope="col" className="px-2 py-1.5 font-medium text-text-muted">
                        {LEVEL_WORD[level]}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {calibration.data.kinds.map((kind) => (
                    <tr key={kind.claimKind}>
                      <th
                        scope="row"
                        className="border-t border-border-subtle px-2 py-2 align-top font-medium text-text-primary"
                      >
                        {kindWord(kind.claimKind)}
                        <span className="block font-normal text-text-muted">
                          {kind.lowestAccepted
                            ? `Alone from ${LEVEL_WORD[kind.lowestAccepted].toLowerCase()}`
                            : 'Only corroboration'}
                        </span>
                      </th>
                      {kind.levels.map((bar) => (
                        <BarCell key={bar.level} bar={bar} />
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <h3 className="mt-5 text-xs font-semibold uppercase tracking-[0.06em] text-text-tertiary">
              Automatic changes
            </h3>
            {changes.length === 0 ? (
              <p className="mt-1 text-sm text-text-secondary">
                None: every bar is the design’s table.
              </p>
            ) : (
              <ul className="mt-1 list-disc space-y-1 pl-5 text-sm text-text-secondary">
                {changes.map((change) => (
                  <li key={`${change.claimKind}-${change.level}-${change.at}-${change.claimId}`}>
                    {change.reason.text}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </SettingsSection>

      {calibration.data && (
        <SettingsSection
          title="Policy"
          description="The stated parameters these rules compare against, and where each comes from."
        >
          <dl className="divide-y divide-border-subtle text-sm">
            {calibration.data.parameters.map((parameter) => (
              <div key={parameter.name} className="py-3">
                <dt className="font-mono text-xs text-text-primary">
                  {parameter.name} = {parameter.value}
                </dt>
                <dd className="mt-0.5 text-xs leading-relaxed text-text-secondary">
                  {parameter.derivation}
                </dd>
              </div>
            ))}
          </dl>
        </SettingsSection>
      )}

      <SettingsSection
        title="Checker reliability"
        description="How often the local checker was wrong on statements you labelled, per checker model."
      >
        <div className="space-y-2 py-3">
          {checker.isLoading && (
            <p role="status" className="text-sm text-text-muted">
              Loading…
            </p>
          )}
          {checker.error && (
            <p role="alert" className="text-sm text-danger-fg">
              {checker.error.message}
            </p>
          )}
          {checker.data?.length === 0 && (
            <p className="text-sm text-text-secondary">
              No labelled statements yet. Each confirmation, removal, edit or added source labels
              one.
            </p>
          )}
          {checker.data?.map((row) => (
            <p key={row.checkerModel} className="text-sm text-text-secondary">
              <span className="font-medium text-text-primary">{row.checkerModel}: </span>
              {accuracyLine(row)}
            </p>
          ))}
        </div>
      </SettingsSection>

      <SettingsSection
        title="Evaluation"
        description="Your overrides as labelled examples for the provenance eval (evals/provenance)."
      >
        <div className="py-3">
          <Button
            type="button"
            variant="outline"
            disabled={exported.isPending}
            onClick={() => exported.mutate()}
          >
            {exported.isPending ? 'Exporting…' : 'Export provenance eval'}
          </Button>
          {exported.data && (
            <p role="status" className="mt-2 text-sm text-text-secondary">
              Saved {exported.data.examples}{' '}
              {exported.data.examples === 1 ? 'example' : 'examples'} to {exported.data.path}.
            </p>
          )}
          {exported.isSuccess && exported.data === null && (
            <p role="status" className="mt-2 text-sm text-text-secondary">
              Export cancelled.
            </p>
          )}
          {exported.error && (
            <p role="alert" className="mt-2 text-sm text-danger-fg">
              {exported.error.message}
            </p>
          )}
        </div>
      </SettingsSection>
    </>
  );
}
