import { settingsFieldClass } from '@/components/ui/SettingsSection';
import type { SourceGroup } from '@/lib/bindings';

interface Props {
  value: SourceGroup | null;
  onChange: (value: SourceGroup | null) => void;
  disabled?: boolean;
}

export function RelatedSourceOptions({ value, onChange, disabled }: Props) {
  return (
    <fieldset disabled={disabled} className="mt-6 space-y-4 rounded-md border border-border-subtle p-4">
      <legend className="px-1 text-sm font-medium">How do these files relate?</legend>
      <label className="flex items-center gap-2 text-sm">
        <input type="checkbox" checked={Boolean(value)} onChange={event => onChange(event.target.checked ? {
          id: crypto.randomUUID(), title: '', edition: null, description: null, ordered: true, structure: 'sections',
        } : null)} />
        These files belong to the same book, manual, or source
      </label>
      <p className="text-xs text-text-muted">Shared context helps search understand each passage. Spaces and collections control where you organize and use the files.</p>
      {value && <>
        <div className="grid gap-4 sm:grid-cols-2">
          <label className="space-y-1 text-sm">Source title <span className="text-text-muted">(required)</span>
            <input aria-label="Source title" maxLength={160} placeholder="e.g. Manual of Patent Examining Procedure" className={`${settingsFieldClass} w-full`} value={value.title} onChange={event => onChange({ ...value, title: event.target.value })} />
          </label>
          <label className="space-y-1 text-sm">Edition or version <span className="text-text-muted">(optional)</span>
            <input aria-label="Edition or version" maxLength={80} placeholder="e.g. Revision 01.2024" className={`${settingsFieldClass} w-full`} value={value.edition ?? ''} onChange={event => onChange({ ...value, edition: event.target.value || null })} />
          </label>
        </div>
        <label className="block space-y-1 text-sm">What connects these files? <span className="text-text-muted">(optional)</span>
          <textarea aria-label="Shared source description" maxLength={500} rows={2} placeholder="e.g. Chapters and appendices from one manual" className={`${settingsFieldClass} w-full`} value={value.description ?? ''} onChange={event => onChange({ ...value, description: event.target.value || null })} />
        </label>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={value.ordered} onChange={event => onChange({ ...value, ordered: event.target.checked })} />
          Read these files in the order shown below
        </label>
        <label className="block space-y-1 text-sm">Split passages by
          <select aria-label="Split passages by" className={`${settingsFieldClass} ml-3`} value={value.structure} onChange={event => onChange({ ...value, structure: event.target.value as SourceGroup['structure'] })}>
            <option value="sections">Sections and headings (recommended)</option>
            <option value="pages">Pages — for slides or inconsistent headings</option>
          </select>
        </label>
        <p className="text-xs text-text-muted">PDF page boundaries are kept in both modes. Source text stays separate from your description so citations point to the file.</p>
      </>}
    </fieldset>
  );
}
