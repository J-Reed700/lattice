import { Check, Settings2 } from 'lucide-react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

import { type DensityMode, type GroupByMode } from './hooks/useCorpusBrowser';
import { type SortField, type SortOrder } from '../../types/fileBrowser';

interface CorpusDisplayMenuProps {
  groupBy: GroupByMode;
  onGroupByChange: (mode: GroupByMode) => void;
  sortField: SortField;
  sortOrder: SortOrder;
  onSortChange: (field: SortField, order: SortOrder) => void;
  density: DensityMode;
  onDensityChange: (density: DensityMode) => void;
}

const GROUP_OPTIONS: { value: GroupByMode; label: string }[] = [
  { value: 'none', label: 'None' },
  { value: 'date', label: 'Date' },
  { value: 'type', label: 'Type' },
  { value: 'folder', label: 'Folder' },
  { value: 'source', label: 'Source' },
];

type SortOption = { value: string; label: string; field: SortField; order: SortOrder };

const SORT_OPTIONS: SortOption[] = [
  { value: 'modified:desc', label: 'Recent first', field: 'modified', order: 'desc' },
  { value: 'modified:asc', label: 'Oldest first', field: 'modified', order: 'asc' },
  { value: 'name:asc', label: 'Name A–Z', field: 'name', order: 'asc' },
  { value: 'name:desc', label: 'Name Z–A', field: 'name', order: 'desc' },
  { value: 'size:desc', label: 'Largest first', field: 'size', order: 'desc' },
  { value: 'size:asc', label: 'Smallest first', field: 'size', order: 'asc' },
];

const DENSITY_OPTIONS: { value: DensityMode; label: string }[] = [
  { value: 'list', label: 'List' },
  { value: 'detail', label: 'Detail' },
];

/**
 * Display menu — §15.4 single dropdown containing Group-by, Sort, and Density
 * sections. Replaces the three separate controls from the original §5.1.
 */
export function CorpusDisplayMenu({
  groupBy,
  onGroupByChange,
  sortField,
  sortOrder,
  onSortChange,
  density,
  onDensityChange,
}: CorpusDisplayMenuProps) {
  const currentSortValue = `${sortField}:${sortOrder}`;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="Display options"
          className="inline-flex h-8 items-center gap-1.5 rounded-[var(--radius-sm)] px-2.5 text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
        >
          <Settings2 className="h-3.5 w-3.5" strokeWidth={1.75} />
          <span>Display</span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" sideOffset={6} className="w-64 p-0">
        <Section label="Group by">
          {GROUP_OPTIONS.map((option) => (
            <MenuRow
              key={option.value}
              label={option.label}
              active={groupBy === option.value}
              onClick={() => onGroupByChange(option.value)}
            />
          ))}
        </Section>

        <Divider />

        <Section label="Sort">
          {SORT_OPTIONS.map((option) => (
            <MenuRow
              key={option.value}
              label={option.label}
              active={currentSortValue === option.value}
              onClick={() => onSortChange(option.field, option.order)}
            />
          ))}
        </Section>

        <Divider />

        <Section label="Density">
          {DENSITY_OPTIONS.map((option) => (
            <MenuRow
              key={option.value}
              label={option.label}
              active={density === option.value}
              onClick={() => onDensityChange(option.value)}
            />
          ))}
        </Section>
      </PopoverContent>
    </Popover>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="px-1.5 py-1">
      <div className="px-2 py-1 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
        {label}
      </div>
      <div className="flex flex-col">{children}</div>
    </div>
  );
}

function Divider() {
  return <div className="my-0.5 h-px bg-[hsl(var(--border-subtle))]" aria-hidden="true" />;
}

function MenuRow({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-7 w-full items-center justify-between rounded-[var(--radius-sm)] px-2 text-sm transition-colors duration-fast ${
        active
          ? 'bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))]'
          : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]'
      }`}
    >
      <span>{label}</span>
      {active && <Check className="h-3.5 w-3.5 text-[hsl(var(--accent))]" strokeWidth={1.75} />}
    </button>
  );
}
