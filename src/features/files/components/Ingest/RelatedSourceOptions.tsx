import { Checkbox, Input, Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Textarea } from '@/components/ui';
import type { SourceGroup } from '@/lib/bindings';

interface Props {
  value: SourceGroup | null;
  onChange: (value: SourceGroup | null) => void;
  disabled?: boolean;
}

export function RelatedSourceOptions({ value, onChange, disabled }: Props) {
  return (
    <fieldset
      disabled={disabled}
      className="mt-6 space-y-4 rounded-xl border border-border-subtle bg-bg/50 p-4"
    >
      <legend className="px-1.5 text-xs font-medium uppercase tracking-[0.06em] text-text-muted">
        How do these files relate?
      </legend>
      <Checkbox
        checked={Boolean(value)}
        disabled={disabled}
        label="These files belong to the same book, manual, or source"
        description="Shared context helps search understand each passage. Spaces and collections control where you organize and use the files."
        onCheckedChange={(checked) =>
          onChange(
            checked
              ? {
                  id: crypto.randomUUID(),
                  title: '',
                  edition: null,
                  description: null,
                  ordered: true,
                  structure: 'sections',
                }
              : null,
          )
        }
      />
      {value && (
        <div className="space-y-4 border-t border-border-subtle pt-4">
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <span className="block text-xs font-medium text-text-secondary">
                Source title <span className="font-normal text-text-muted">(required)</span>
              </span>
              <Input
                aria-label="Source title"
                placeholder="e.g. Manual of Patent Examining Procedure"
                value={value.title}
                disabled={disabled}
                onChange={(event) => onChange({ ...value, title: event.target.value })}
              />
            </div>
            <div className="space-y-1.5">
              <span className="block text-xs font-medium text-text-secondary">
                Edition or version <span className="font-normal text-text-muted">(optional)</span>
              </span>
              <Input
                aria-label="Edition or version"
                placeholder="e.g. Revision 01.2024"
                value={value.edition ?? ''}
                disabled={disabled}
                onChange={(event) => onChange({ ...value, edition: event.target.value || null })}
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <span className="block text-xs font-medium text-text-secondary">
              What connects these files? <span className="font-normal text-text-muted">(optional)</span>
            </span>
            <Textarea
              aria-label="Shared source description"
              rows={2}
              placeholder="e.g. Chapters and appendices from one manual"
              value={value.description ?? ''}
              disabled={disabled}
              onChange={(event) => onChange({ ...value, description: event.target.value || null })}
              className="min-h-[64px]"
            />
          </div>

          <Checkbox
            checked={value.ordered}
            disabled={disabled}
            label="Read these files in the order shown below"
            onCheckedChange={(checked) => onChange({ ...value, ordered: checked })}
          />

          <div className="flex flex-wrap items-center gap-3">
            <span className="text-xs font-medium text-text-secondary">Split passages by</span>
            <Select
              value={value.structure}
              disabled={disabled}
              onValueChange={(structure) =>
                onChange({ ...value, structure: structure as SourceGroup['structure'] })
              }
            >
              <SelectTrigger aria-label="Split passages by" className="w-[300px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="sections">Sections and headings (recommended)</SelectItem>
                <SelectItem value="pages">Pages — for slides or inconsistent headings</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <p className="text-xs text-text-muted">
            PDF page boundaries are kept in both modes. Source text stays separate from your description so citations point to the file.
          </p>
        </div>
      )}
    </fieldset>
  );
}
