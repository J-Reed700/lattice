import { Checkbox } from '@/components/ui/Checkbox';
import type { LearningPackExportOptionsDto } from '@/lib/bindings';

/** The export choices exactly as every export request nests them under `options`. */
export type PackExportOptionsValue = LearningPackExportOptionsDto;

/** Defaults for moving a course: progress and lab work travel, full source text stays opt-in. */
export const MOVE_EXPORT_DEFAULTS: PackExportOptionsValue = {
  includeEvidence: true,
  includePracticalArtifacts: true,
  includeSourceBodies: false,
  sourceBodyRedistributionConfirmed: false,
};

/** Full source text needs the learner's explicit redistribution tick. */
export const packOptionsReady = (value: PackExportOptionsValue) => !value.includeSourceBodies || value.sourceBodyRedistributionConfirmed;

/** The request's `options`, with the redistribution tick only ever sent alongside full text. */
export const packOptionsRequest = (value: PackExportOptionsValue): PackExportOptionsValue => ({
  ...value,
  sourceBodyRedistributionConfirmed: value.includeSourceBodies && value.sourceBodyRedistributionConfirmed,
});

export function PackExportOptions({ value, onChange }: { value: PackExportOptionsValue; onChange: (next: PackExportOptionsValue) => void }) {
  const set = (patch: Partial<PackExportOptionsValue>) => onChange({ ...value, ...patch });
  return <>
    <p className="mt-3 rounded-lg border border-warning/20 bg-warning-muted/40 p-3 text-xs leading-5 text-warning-fg">Assessment answer keys are included in the pack so restored questions still work. They are not shown in normal app views, but anyone who can inspect the pack file can read them. Hidden practical evaluator files are excluded.</p>
    <div className="mt-4 space-y-2">
      <div className="rounded-lg border border-border-subtle bg-bg p-3">
        <Checkbox
          checked={value.includeEvidence}
          onCheckedChange={(checked) => set({ includeEvidence: checked })}
          label="Include learner evidence"
          description="Includes submitted responses, recall and review history, program tutor turns, workbench history, and Canvas documents. Restorable assessments include their model-authored answer keys; hidden evaluators remain out."
        />
      </div>
      <div className="rounded-lg border border-border-subtle bg-bg p-3">
        <Checkbox
          checked={value.includePracticalArtifacts}
          onCheckedChange={(checked) => set({ includePracticalArtifacts: checked })}
          label="Include practical work artifacts"
          description="Includes authored activities and saved learner files; hidden evaluator files remain out."
        />
      </div>
      <div className="rounded-lg border border-border-subtle bg-bg p-3">
        <Checkbox
          checked={value.includeSourceBodies}
          onCheckedChange={(checked) => set({ includeSourceBodies: checked, ...(checked ? {} : { sourceBodyRedistributionConfirmed: false }) })}
          label="Include full source text"
          description="Only select this if you have rights to redistribute every included source."
        />
      </div>
      {value.includeSourceBodies
        ? <div className="ml-6 rounded-lg border border-warning/25 bg-warning-muted/40 p-3">
          <Checkbox
            checked={value.sourceBodyRedistributionConfirmed}
            onCheckedChange={(checked) => set({ sourceBodyRedistributionConfirmed: checked })}
            label="I confirm that full source bodies in this pack may be redistributed."
          />
        </div>
        : <p className="px-1 text-xs leading-5 text-text-muted">Without full source text, the other computer gets excerpts only; web references can be refreshed there.</p>}
      <p className="rounded-lg border border-border-subtle bg-surface-sunken p-3 text-xs leading-5 text-text-muted">The privacy manifest records whether answer keys, hidden evaluators, learner evidence, practical artifacts, and full source bodies are included. Credentials and Lattice chat outside this program are excluded.</p>
    </div>
  </>;
}
