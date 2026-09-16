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
  { value: 'journal', label: 'From Journal' },
  { value: 'document', label: 'From documents' },
];

/**
 * Sidebar origin filter. One-line trigger matching the Chat scope line.
 */
export function ReferenceOriginPicker({ value, onChange }: ReferenceOriginPickerProps) {
  return (
    <Select value={value} onValueChange={(next) => onChange(next as OriginFilter)}>
      <SelectTrigger
        className="h-7 rounded-sm border-0 px-1 py-0.5 text-sm transition-colors duration-fast hover:bg-surface-raised"
        aria-label="Filter by origin"
      >
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
