import { useEffect, useEffectEvent, useId, useMemo, useState } from 'react';

import { ChevronDown } from 'lucide-react';

import type { ChoiceOption } from './practiceChoices';

export interface ChoiceSlot {
  id: string;
  label: string;
  options: ChoiceOption[];
  defaultValue: string;
  /** Said under the menu to a screen reader when its chosen option has no description of its own. */
  hint?: string;
}

export type SentencePart = string | { slot: string };

export interface StartSentenceProps {
  programId: string;
  cardId: string;
  /** Names the group of choices. */
  legend: string;
  slots: ChoiceSlot[];
  parts: SentencePart[] | ((values: Record<string, string>) => SentencePart[]);
  /** The id of the element that says what the card promises. */
  describedBy?: string;
  onChange?: (values: Record<string, string>) => void;
}

const storageKey = (programId: string, cardId: string) => `lattice:practice:sentence:${programId}:${cardId}`;

function readChosen(programId: string, cardId: string): Record<string, string> {
  try {
    const raw = localStorage.getItem(storageKey(programId, cardId));
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (!parsed || typeof parsed !== 'object') return {};
    return Object.fromEntries(Object.entries(parsed).filter((entry): entry is [string, string] => typeof entry[1] === 'string'));
  } catch {
    return {};
  }
}

function writeChosen(programId: string, cardId: string, chosen: Record<string, string>): void {
  try {
    localStorage.setItem(storageKey(programId, cardId), JSON.stringify(chosen));
  } catch {
    // Storage may be full or off; the choice then lasts until the page closes.
  }
}

/** The choices of a card as one sentence, each a menu, remembered for the course. */
export function lastChosen(programId: string, cardId: string, slotId: string): string | null {
  return readChosen(programId, cardId)[slotId] ?? null;
}

export function StartSentence({ programId, cardId, legend, slots, parts, describedBy, onChange }: StartSentenceProps) {
  const uid = useId();
  const [chosen, setChosen] = useState<Record<string, string>>(() => readChosen(programId, cardId));

  useEffect(() => {
    setChosen(readChosen(programId, cardId));
  }, [programId, cardId]);

  const values = useMemo(() => {
    const result: Record<string, string> = {};
    for (const slot of slots) {
      const picked = chosen[slot.id];
      result[slot.id] = picked !== undefined && slot.options.some((option) => option.value === picked) ? picked : slot.defaultValue;
    }
    return result;
  }, [slots, chosen]);

  const signature = JSON.stringify(values);
  // The parent hears of a change by its value, not by the callback's identity.
  const announce = useEffectEvent((next: string) => {
    onChange?.(JSON.parse(next) as Record<string, string>);
  });
  useEffect(() => {
    announce(signature);
  }, [signature]);

  const pick = (slotId: string, value: string) => {
    const next = { ...chosen, [slotId]: value };
    setChosen(next);
    writeChosen(programId, cardId, next);
  };

  const sentence = typeof parts === 'function' ? parts(values) : parts;

  return (
    <fieldset aria-describedby={describedBy} className="m-0 min-w-0 border-0 p-0">
      <legend className="sr-only">{legend}</legend>
      <p className="m-0 text-sm leading-8 text-text-primary">
        {sentence.map((part, index) => {
          if (typeof part === 'string') return <span key={`t${index}`}>{part}</span>;
          const slot = slots.find((entry) => entry.id === part.slot);
          if (!slot) return null;
          const current = slot.options.find((option) => option.value === values[slot.id]) ?? slot.options[0];
          if (slot.options.length === 1) {
            return (
              <strong key={slot.id} className="font-semibold text-accent">
                {current?.label}
              </strong>
            );
          }
          const descriptionId = `${uid}-${slot.id}`;
          return (
            <span key={slot.id} className="relative mx-0.5 inline-flex items-center">
              <select
                aria-label={slot.label}
                aria-describedby={descriptionId}
                value={values[slot.id]}
                onChange={(event) => pick(slot.id, event.target.value)}
                className="min-h-8 cursor-pointer appearance-none rounded-md border border-border-default bg-accent-muted py-0.5 pl-2 pr-6 text-sm font-semibold text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              >
                {slot.options.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              <ChevronDown aria-hidden="true" className="pointer-events-none absolute right-1.5 h-3.5 w-3.5 text-accent" />
              <span id={descriptionId} className="sr-only">
                {current?.description ?? slot.hint ?? slot.label}
              </span>
            </span>
          );
        })}
      </p>
    </fieldset>
  );
}
