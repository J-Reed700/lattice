import { useEffect, useState } from 'react';
import { ChevronDown } from 'lucide-react';

import { cn } from '@/lib/utils';

export interface ChoiceOption {
  value: string;
  label: string;
  description?: string;
}

export interface ChoiceSlot {
  id: string;
  label: string;
  value: string;
  options: ChoiceOption[];
  ariaDescribedBy?: string;
}

export interface StartSentenceProps {
  programId: string;
  cardId: 'explain' | 'spot' | 'code' | 'mock' | 'role';
  slots: ChoiceSlot[];
  sentencePrefix?: string;
  sentenceSuffix?: string;
  onChange?: (slotId: string, value: string) => void;
  className?: string;
}

function getStoredSelections(
  programId: string,
  cardId: string,
): Record<string, string> {
  try {
    const raw = localStorage.getItem(
      `lattice:practice:startSentence:${programId}:${cardId}`,
    );
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function saveStoredSelections(
  programId: string,
  cardId: string,
  selections: Record<string, string>,
): void {
  try {
    localStorage.setItem(
      `lattice:practice:startSentence:${programId}:${cardId}`,
      JSON.stringify(selections),
    );
  } catch {
    // Storage quota or disabled storage is ignored.
  }
}

export function StartSentence({
  programId,
  cardId,
  slots,
  sentencePrefix,
  sentenceSuffix,
  onChange,
  className,
}: StartSentenceProps) {
  const [selections, setSelections] = useState<Record<string, string>>(() => {
    const stored = getStoredSelections(programId, cardId);
    const initial: Record<string, string> = {};
    for (const slot of slots) {
      initial[slot.id] = stored[slot.id] ?? slot.value;
    }
    return initial;
  });

  const [openSlotId, setOpenSlotId] = useState<string | null>(null);

  useEffect(() => {
    const stored = getStoredSelections(programId, cardId);
    setSelections((current) => {
      const updated: Record<string, string> = { ...current };
      for (const slot of slots) {
        if (stored[slot.id] !== undefined) {
          updated[slot.id] = stored[slot.id];
        } else if (updated[slot.id] === undefined) {
          updated[slot.id] = slot.value;
        }
      }
      return updated;
    });
  }, [programId, cardId, slots]);

  const handleSelect = (slotId: string, value: string) => {
    const next = { ...selections, [slotId]: value };
    setSelections(next);
    saveStoredSelections(programId, cardId, next);
    setOpenSlotId(null);
    onChange?.(slotId, value);
  };

  return (
    <fieldset
      role="group"
      className={cn(
        'm-0 inline border-0 p-0 text-sm leading-relaxed text-text-primary',
        className,
      )}
    >
      <legend className="sr-only">
        {`Practice options for ${cardId}`}
      </legend>
      {sentencePrefix && <span>{sentencePrefix}</span>}
      {slots.map((slot, index) => {
        const currentValue = selections[slot.id] ?? slot.value;
        const currentOption = slot.options.find(
          (opt) => opt.value === currentValue,
        ) ?? slot.options[0];
        const isOpen = openSlotId === slot.id;

        return (
          <span key={slot.id} className="relative inline-block">
            <button
              type="button"
              aria-haspopup="listbox"
              aria-expanded={isOpen}
              aria-label={`${slot.label}: ${currentOption?.label ?? currentValue}`}
              aria-describedby={slot.ariaDescribedBy}
              onClick={() => setOpenSlotId(isOpen ? null : slot.id)}
              className="mx-0.5 inline-flex min-h-[28px] cursor-pointer items-center gap-0.5 rounded-md bg-accent-muted px-1.5 py-0.5 font-semibold text-accent transition-colors hover:opacity-85 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span>{currentOption?.label ?? currentValue}</span>
              <ChevronDown className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            </button>
            {slot.ariaDescribedBy && (
              <span id={slot.ariaDescribedBy} className="sr-only">
                {currentOption?.description ?? slot.label}
              </span>
            )}
            {isOpen && (
              <ul
                role="listbox"
                aria-label={slot.label}
                className="absolute top-full left-0 z-30 m-0 mt-1 min-w-[160px] list-none rounded-lg border border-border-default bg-surface p-1 shadow-md focus:outline-hidden"
              >
                {slot.options.map((opt) => {
                  const isSelected = opt.value === currentValue;
                  return (
                    <li
                      key={opt.value}
                      role="option"
                      aria-selected={isSelected}
                      onClick={() => handleSelect(slot.id, opt.value)}
                      className={cn(
                        'flex min-h-[36px] cursor-pointer items-center justify-between rounded-md px-2.5 py-1.5 text-xs font-medium text-text-primary hover:bg-surface-raised focus:bg-surface-raised',
                        isSelected && 'bg-surface-raised font-semibold text-accent',
                      )}
                    >
                      <div className="flex flex-col">
                        <span>{opt.label}</span>
                        {opt.description && (
                          <span className="text-xxs text-text-muted">
                            {opt.description}
                          </span>
                        )}
                      </div>
                      {isSelected && (
                        <span className="text-xs text-accent" aria-hidden="true">
                          ✓
                        </span>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
            {index < slots.length - 1 && ' '}
          </span>
        );
      })}
      {sentenceSuffix && <span>{sentenceSuffix}</span>}
    </fieldset>
  );
}
