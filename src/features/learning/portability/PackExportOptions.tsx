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
    <p className="mt-3 rounded-lg bg-amber-500/10 p-3 text-[10px] leading-4 text-amber-800">Assessment answer keys are included in the pack so restored questions still work. They are not shown in normal app views, but anyone who can inspect the pack file can read them. Hidden practical evaluator files are excluded.</p>
    <div className="mt-4 space-y-2">
      <label className="flex gap-2 rounded-lg border border-border bg-background p-3 text-xs text-text-secondary"><input type="checkbox" checked={value.includeEvidence} onChange={(event) => set({ includeEvidence: event.target.checked })} className="mt-0.5 accent-accent" /><span><strong className="font-medium text-text-primary">Include learner evidence</strong><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">Includes submitted responses, recall and review history, program tutor turns, workbench history, and Canvas documents. Restorable assessments include their model-authored answer keys; hidden evaluators remain out.</span></span></label>
      <label className="flex gap-2 rounded-lg border border-border bg-background p-3 text-xs text-text-secondary"><input type="checkbox" checked={value.includePracticalArtifacts} onChange={(event) => set({ includePracticalArtifacts: event.target.checked })} className="mt-0.5 accent-accent" /><span><strong className="font-medium text-text-primary">Include practical work artifacts</strong><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">Includes authored activities and saved learner files; hidden evaluator files remain out.</span></span></label>
      <label className="flex gap-2 rounded-lg border border-border bg-background p-3 text-xs text-text-secondary"><input type="checkbox" checked={value.includeSourceBodies} onChange={(event) => set({ includeSourceBodies: event.target.checked, ...(event.target.checked ? {} : { sourceBodyRedistributionConfirmed: false }) })} className="mt-0.5 accent-accent" /><span><strong className="font-medium text-text-primary">Include full source text</strong><span className="mt-0.5 block text-[10px] leading-4 text-text-muted">Only select this if you have rights to redistribute every included source.</span></span></label>
      {value.includeSourceBodies
        ? <label className="ml-6 flex items-start gap-2 rounded-lg bg-amber-500/10 p-3 text-xs text-text-secondary"><input type="checkbox" checked={value.sourceBodyRedistributionConfirmed} onChange={(event) => set({ sourceBodyRedistributionConfirmed: event.target.checked })} className="mt-0.5 accent-accent" /><span>I confirm that full source bodies in this pack may be redistributed.</span></label>
        : <p className="px-1 text-[10px] leading-4 text-text-muted">Without full source text, the other computer gets excerpts only; web references can be refreshed there.</p>}
      <p className="rounded-lg border border-border bg-background/60 p-3 text-[10px] leading-4 text-text-muted">The privacy manifest records whether answer keys, hidden evaluators, learner evidence, practical artifacts, and full source bodies are included. Credentials and Lattice chat outside this program are excluded.</p>
    </div>
  </>;
}
