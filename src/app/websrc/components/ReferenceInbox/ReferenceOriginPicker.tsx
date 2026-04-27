import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

import type { OriginFilter } from './useReferenceInbox';

interface ReferenceOriginPickerProps {
  value: OriginFilter;
  onChange: (value: OriginFilter) => void;
}

const OPTIONS: Array<{ value: OriginFilter; label: string }> = [
  { value: 'all', label: 'All references' },
  { value: 'chat', label: 'From Chat' },
  { value: 'journal', label: 'From Journal entries' },
];

/**
 * Sidebar origin filter: shadcn Select with three options.
 * Replaces the prior role-tab rail (All / AI / You / System).
 * Spec §4.1.2, §6.3.
 */
export function ReferenceOriginPicker({ value, onChange }: ReferenceOriginPickerProps) {
  return (
    <Select value={value} onValueChange={(next) => onChange(next as OriginFilter)}>
      <SelectTrigger className="h-9 text-sm" aria-label="Filter by origin">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {OPTIONS.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
