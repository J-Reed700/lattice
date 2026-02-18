import { useState, useRef, useEffect } from 'react';

import { format } from 'date-fns';
import { Calendar, X } from 'lucide-react';
import { DayPicker } from 'react-day-picker';
import 'react-day-picker/dist/style.css';

/**
 * DatePicker
 *
 * Purpose: Allow users to select dates with a calendar interface
 *
 * Features:
 * - Calendar popover on click
 * - Date formatting with date-fns
 * - Keyboard navigation
 * - Clear button for resetting selection
 * - Today button for quick selection
 * - Min/max date constraints
 * - Click outside to close
 *
 * States: closed, open, selected, disabled
 * Accessibility: WCAG AA, keyboard navigation, ARIA labeling
 */

export interface DatePickerProps {
  selected?: Date;
  onSelect: (date: Date | undefined) => void;
  disabled?: boolean;
  placeholder?: string;
  minDate?: Date;
  maxDate?: Date;
  className?: string;
  dateFormat?: string;
}

export function DatePicker({
  selected,
  onSelect,
  disabled = false,
  placeholder = 'Select date',
  minDate,
  maxDate,
  className = '',
  dateFormat = 'MMM dd, yyyy',
}: DatePickerProps) {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Close popover when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && event.target instanceof Node && !containerRef.current.contains(event.target)) {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => {
        document.removeEventListener('mousedown', handleClickOutside);
      };
    }
  }, [isOpen]);

  // Close on Escape key
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleEscape);
      return () => {
        document.removeEventListener('keydown', handleEscape);
      };
    }
  }, [isOpen]);

  const handleSelect = (date: Date | undefined) => {
    onSelect(date);
    setIsOpen(false);
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect(undefined);
  };

  const handleToday = () => {
    onSelect(new Date());
    setIsOpen(false);
  };

  const displayValue = selected ? format(selected, dateFormat) : placeholder;

  return (
    <div className={`relative inline-block ${className}`} ref={containerRef}>
      {/* Input trigger */}
      <button
        type="button"
        onClick={() => !disabled && setIsOpen(!isOpen)}
        disabled={disabled}
        className="
          inline-flex
          items-center
          justify-between
          w-full
          px-3
          py-2
          text-sm
          bg-[var(--surface-primary)]
          border
          border-[var(--border-color)]
          rounded-lg
          shadow-sm
          hover:bg-[var(--surface-hover)]
          focus:outline-none
          focus-visible:ring-2
          focus-visible:ring-[var(--accent-primary)]
          focus-visible:ring-offset-1
          disabled:opacity-50
          disabled:cursor-not-allowed
          transition-colors
          duration-150
        "
        aria-haspopup="dialog"
        aria-expanded={isOpen}
        aria-label="Choose date"
      >
        <span className="flex items-center gap-2">
          <Calendar className="h-4 w-4 text-[var(--text-secondary)]" />
          <span className={selected ? 'text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'}>
            {displayValue}
          </span>
        </span>
        {selected && !disabled && (
          <button
            type="button"
            onClick={handleClear}
            className="
              ml-2
              text-[var(--text-tertiary)]
              hover:text-[var(--text-secondary)]
              transition-colors
              focus:outline-none
              focus-visible:ring-2
              focus-visible:ring-[var(--accent-primary)]
              rounded
            "
            aria-label="Clear date"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </button>

      {/* Calendar popover */}
      {isOpen && (
        <div
          className="
            absolute
            z-50
            mt-2
            bg-[var(--surface-primary)]
            border
            border-[var(--border-color)]
            rounded-lg
            shadow-lg
            p-3
            animate-in
            fade-in-0
            zoom-in-95
          "
          role="dialog"
          aria-label="Date picker calendar"
        >
          <DayPicker
            mode="single"
            selected={selected}
            onSelect={handleSelect}
            disabled={[
              minDate ? { before: minDate } : false,
              maxDate ? { after: maxDate } : false,
            ].filter((x): x is { before: Date } | { after: Date } => x !== false)}
            className="date-picker-calendar"
            classNames={{
              months: 'flex flex-col space-y-4',
              month: 'space-y-4',
              caption: 'flex justify-center pt-1 relative items-center',
              caption_label: 'text-sm font-medium text-[var(--text-primary)]',
              nav: 'space-x-1 flex items-center',
              nav_button: 'h-7 w-7 bg-transparent p-0 hover:bg-[var(--surface-hover)] rounded-md transition-colors',
              nav_button_previous: 'absolute left-1',
              nav_button_next: 'absolute right-1',
              table: 'w-full border-collapse space-y-1',
              head_row: 'flex',
              head_cell: 'text-[var(--text-secondary)] rounded-md w-9 font-normal text-[0.8rem]',
              row: 'flex w-full mt-2',
              cell: 'text-center text-sm p-0 relative [&:has([aria-selected])]:bg-[var(--accent-light)] first:[&:has([aria-selected])]:rounded-l-md last:[&:has([aria-selected])]:rounded-r-md focus-within:relative focus-within:z-20',
              day: 'h-9 w-9 p-0 font-normal hover:bg-[var(--surface-hover)] rounded-md transition-colors aria-selected:bg-[var(--accent-primary)] aria-selected:text-white aria-selected:hover:bg-[var(--accent-hover)] aria-selected:focus:bg-[var(--accent-primary)]',
              day_selected: 'bg-[var(--accent-primary)] text-white hover:bg-[var(--accent-hover)] focus:bg-[var(--accent-primary)]',
              day_today: 'bg-[var(--bg-tertiary)] font-semibold',
              day_outside: 'text-[var(--text-tertiary)] opacity-50',
              day_disabled: 'text-[var(--text-tertiary)] opacity-50 cursor-not-allowed',
              day_hidden: 'invisible',
            }}
          />

          {/* Quick action buttons */}
          <div className="flex items-center justify-between gap-2 mt-3 pt-3 border-t border-[var(--border-color)]">
            <button
              type="button"
              onClick={handleToday}
              className="
                px-3
                py-1.5
                text-xs
                font-medium
                text-[var(--accent-primary)]
                hover:text-[var(--accent-primary)]
                hover:bg-[var(--accent-light)]
                rounded
                transition-colors
                focus:outline-none
                focus-visible:ring-2
                focus-visible:ring-[var(--accent-primary)]
              "
            >
              Today
            </button>
            <button
              type="button"
              onClick={() => setIsOpen(false)}
              className="
                px-3
                py-1.5
                text-xs
                font-medium
                text-[var(--text-secondary)]
                hover:text-[var(--text-primary)]
                hover:bg-[var(--surface-hover)]
                rounded
                transition-colors
                focus:outline-none
                focus-visible:ring-2
                focus-visible:ring-[var(--text-secondary)]
              "
            >
              Close
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
