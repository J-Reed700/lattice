import { useMemo, useState } from 'react';

import { CalendarDays } from 'lucide-react';
import { DayPicker } from 'react-day-picker';

import 'react-day-picker/dist/style.css';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

import type { JournalEntrySummary } from './useJournalEntries';

interface JournalCalendarPopoverProps {
  entries: JournalEntrySummary[];
  onJumpToEntry: (entryId: string) => void;
}

function startOfDay(date: Date): Date {
  const d = new Date(date);
  d.setHours(0, 0, 0, 0);
  return d;
}

/**
 * Small calendar popover anchored to an icon in the sidebar top rail.
 * Dates with at least one entry are marked; clicking jumps to the first
 * entry from that date.
 * Spec §7.
 */
export function JournalCalendarPopover({ entries, onJumpToEntry }: JournalCalendarPopoverProps) {
  const [isOpen, setIsOpen] = useState(false);

  const entryDateMap = useMemo(() => {
    const map = new Map<number, string>();
    // Iterate from oldest to newest so the *first* entry of a day wins.
    const sorted = [...entries].sort((a, b) => {
      const aT = new Date(a.updatedAt).getTime();
      const bT = new Date(b.updatedAt).getTime();
      if (Number.isNaN(aT) || Number.isNaN(bT)) return 0;
      return aT - bT;
    });
    for (const entry of sorted) {
      const d = new Date(entry.updatedAt);
      if (Number.isNaN(d.getTime())) continue;
      const key = startOfDay(d).getTime();
      if (!map.has(key)) map.set(key, entry.id);
    }
    return map;
  }, [entries]);

  const datesWithEntries = useMemo(() => [...entryDateMap.keys()].map((ts) => new Date(ts)), [entryDateMap]);

  const handleSelect = (date: Date | undefined) => {
    if (!date) return;
    const key = startOfDay(date).getTime();
    const direct = entryDateMap.get(key);
    if (direct) {
      onJumpToEntry(direct);
      setIsOpen(false);
      return;
    }
    let best: { id: string; diff: number } | null = null;
    for (const [ts, id] of entryDateMap.entries()) {
      const diff = Math.abs(ts - key);
      if (!best || diff < best.diff) best = { id, diff };
    }
    if (best) {
      onJumpToEntry(best.id);
      setIsOpen(false);
    }
  };

  return (
    <Popover open={isOpen} onOpenChange={setIsOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="rounded-sm p-1.5 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          aria-label="Open date picker"
          title="Jump to date"
        >
          <CalendarDays className="h-4 w-4" strokeWidth={1.75} />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto p-3">
        <DayPicker
          mode="single"
          onSelect={handleSelect}
          modifiers={{ hasEntry: datesWithEntries }}
          modifiersClassNames={{
            hasEntry: 'journal-day-has-entry',
          }}
          classNames={{
            months: 'flex flex-col space-y-4',
            month: 'space-y-4',
            caption: 'flex justify-center pt-1 relative items-center',
            caption_label: 'text-sm font-medium text-[hsl(var(--text-primary))]',
            nav: 'space-x-1 flex items-center',
            nav_button:
              'h-7 w-7 bg-transparent p-0 hover:bg-[hsl(var(--surface))] rounded-sm transition-colors duration-fast text-[hsl(var(--text-tertiary))]',
            nav_button_previous: 'absolute left-1',
            nav_button_next: 'absolute right-1',
            table: 'w-full border-collapse space-y-1',
            head_row: 'flex',
            head_cell:
              'text-[hsl(var(--text-tertiary))] rounded-sm w-9 font-normal text-xxs uppercase tracking-[0.08em]',
            row: 'flex w-full mt-2',
            cell: 'text-center text-sm p-0 relative focus-within:relative focus-within:z-20',
            day: 'h-9 w-9 p-0 font-normal rounded-sm text-[hsl(var(--text-primary))] hover:bg-[hsl(var(--surface))] transition-colors duration-fast',
            day_today:
              'font-semibold text-[hsl(var(--accent-fg))] bg-[hsl(var(--accent))] hover:bg-[hsl(var(--accent-hover))]',
            day_outside: 'text-[hsl(var(--text-muted))] opacity-60',
            day_disabled: 'text-[hsl(var(--text-muted))] opacity-40 cursor-not-allowed',
            day_hidden: 'invisible',
          }}
        />
        <style>{`
          .journal-day-has-entry {
            position: relative;
          }
          .journal-day-has-entry::after {
            content: '';
            position: absolute;
            bottom: 4px;
            left: 50%;
            transform: translateX(-50%);
            width: 4px;
            height: 4px;
            border-radius: 9999px;
            background-color: hsl(var(--accent));
          }
        `}</style>
      </PopoverContent>
    </Popover>
  );
}
