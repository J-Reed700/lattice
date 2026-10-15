import type { ConversationSpaceDto } from '@/types';

interface ImportScopeSelectProps {
  value: string;
  onChange: (_spaceId: string) => void;
  spaces: ConversationSpaceDto[];
  /** Offer "Unscoped" — only when no conversation is open to scope the import to. */
  allowUnscoped: boolean;
  className?: string;
}

/** The space a web source imported from the reader lands in. */
export const ImportScopeSelect = ({
  value,
  onChange,
  spaces,
  allowUnscoped,
  className = '',
}: ImportScopeSelectProps) => (
  <label
    className={`inline-flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))] ${className}`.trim()}
  >
    <span className="text-[hsl(var(--text-muted))]">Scope</span>
    <select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="min-w-48 rounded-sm border border-border-default bg-surface-raised px-2 py-1 text-sm text-[hsl(var(--text-primary))] focus:outline-hidden focus:ring-2 focus:ring-[hsl(var(--ring))]"
    >
      {allowUnscoped && <option value="">Unscoped</option>}
      {spaces.map((space) => (
        <option key={space.id} value={space.id}>
          {space.name}
          {space.isArchived ? ' (archived)' : ''}
        </option>
      ))}
    </select>
  </label>
);
