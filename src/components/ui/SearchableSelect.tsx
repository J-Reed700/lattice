import { useState, type ReactNode } from 'react';

import { Command } from 'cmdk';
import { Check, ChevronsUpDown, Search } from 'lucide-react';

import { cn } from '@/lib/utils';

import { Button } from './button';
import { Popover, PopoverContent, PopoverTrigger } from './popover';
import './interactions.css';

export type SearchableOption = {
  value: string;
  label: string;
  description?: string;
  keywords?: string[];
  indicator?: ReactNode;
};

/** A keyboard-searchable picker; the selected value changes only on confirmation. */
export function SearchableSelect({ id, label, value, options, onValueChange, disabled, placeholder = 'Choose…', searchPlaceholder = 'Search…', emptyMessage = 'No results found.', className }: {
  id?: string;
  label: string;
  value: string;
  options: SearchableOption[];
  onValueChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
  searchPlaceholder?: string;
  emptyMessage?: string;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [host, setHost] = useState<HTMLDivElement | null>(null);
  const selected = options.find((option) => option.value === value);

  return <div ref={setHost} className={cn('ui-searchable-select', className)}>
    <Popover open={open && !disabled} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button id={id} type="button" variant="outline" role="combobox" aria-label={label} aria-expanded={open && !disabled} aria-haspopup="dialog" disabled={disabled} value={value} className="ui-searchable-trigger">
          <span className="ui-searchable-value">{selected?.indicator}<span>{selected?.label ?? placeholder}</span></span>
          <ChevronsUpDown size={14} aria-hidden="true" className="shrink-0 text-text-muted" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" collisionPadding={12} inheritThemeFrom={open ? host : null} className="ui-searchable-popover" aria-label={`${label} picker`}>
        <Command loop label={`Search ${label.toLowerCase()}`}>
          <div className="ui-searchable-input"><Search size={15} aria-hidden="true" /><Command.Input aria-label={`Search ${label.toLowerCase()}`} placeholder={searchPlaceholder} autoFocus /></div>
          <Command.List label={`${label} options`} className="ui-searchable-list">
            <Command.Empty className="ui-searchable-empty">{emptyMessage}</Command.Empty>
            {options.map((option, index) => <Command.Item key={option.value} value={`${index}:${option.value}`} keywords={[option.label, option.description ?? '', ...(option.keywords ?? [])]} onSelect={() => { onValueChange(option.value); setOpen(false); }} className="ui-searchable-option">
              <span className="ui-searchable-indicator">{option.indicator}</span>
              <span className="min-w-0 flex-1"><span className="ui-searchable-option-label">{option.label}</span>{option.description && <span className="ui-searchable-description">{option.description}</span>}</span>
              {option.value === value && <Check size={15} aria-label="Selected" className="shrink-0 text-accent" />}
            </Command.Item>)}
          </Command.List>
          <div className="ui-searchable-footer"><span><kbd>↑</kbd><kbd>↓</kbd> browse</span><span><kbd>↵</kbd> select <kbd>esc</kbd> close</span></div>
        </Command>
      </PopoverContent>
    </Popover>
  </div>;
}
