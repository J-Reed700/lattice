/**
 * TokenLimitField
 *
 * One token limit on a model: a number and, when its top is known, a slider up
 * to it, as a downloaded model's window and a connected model's window and
 * output ceiling all show theirs.
 */

import type { ReactNode } from 'react';

import { HelpCircle } from 'lucide-react';

import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui';

import { NUMBER_FIELD_CLASS, toFinite } from '../AITab/shared';

export const MIN_CONTEXT_LENGTH = 512;

export function HelpTooltip({ content }: { content: string }) {
  return (
    <TooltipProvider delayDuration={150}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            title={content}
            aria-label={`More information: ${content}`}
            className="inline-flex h-4 w-4 cursor-help items-center justify-center rounded-full text-text-muted transition-colors hover:text-text-primary focus:outline-none focus-visible:ring-1 focus-visible:ring-accent"
          >
            <HelpCircle className="h-3.5 w-3.5" />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="top"
          sideOffset={6}
          className="z-[60] max-w-[280px] bg-[hsl(var(--surface-overlay))] px-2.5 py-1.5 text-xs font-normal leading-relaxed text-[hsl(var(--text-primary))] shadow-xl"
        >
          {content}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

interface TokenLimitFieldProps {
  label: string;
  help: string;
  min: number;
  /** The top of the range. Null when nothing states one: the number alone. */
  max: number | null;
  value: number;
  onChange: (value: number) => void;
  /** When the value has settled: the field is left or the slider let go. */
  onCommit?: () => void;
  /** Under the slider, such as a warning about this value. */
  children?: ReactNode;
}

export function TokenLimitField({ label, help, min, max, value, onChange, onCommit, children }: TokenLimitFieldProps) {
  return (
    <div>
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-1.5 text-xs font-medium text-text-primary">
            <span>{label}</span>
            <HelpTooltip content={help} />
          </div>
          {max !== null ? (
            <div className="mt-1 flex items-center gap-1.5 text-xs text-text-muted">
              <span>Max</span>
              <span className="rounded bg-surface px-1.5 py-0.5 font-mono text-[11px] text-text-secondary">
                {max.toLocaleString()}
              </span>
              <span>tokens</span>
            </div>
          ) : null}
        </div>

        <div className="flex items-center gap-2">
          <input
            aria-label={label}
            type="number"
            min={min}
            max={max ?? undefined}
            step={512}
            value={value}
            onChange={(event) => onChange(Math.round(toFinite(event.target.value, value)))}
            onBlur={onCommit}
            className={NUMBER_FIELD_CLASS}
          />
        </div>
      </div>

      {max !== null ? (
        <div className="mt-3">
          <input
            aria-label={`${label} Slider`}
            type="range"
            min={min}
            max={max}
            step={512}
            value={Math.min(Math.max(value, min), max)}
            onChange={(event) => onChange(Number(event.target.value))}
            onPointerUp={onCommit}
            onKeyUp={onCommit}
            className="h-1.5 w-full cursor-pointer appearance-none rounded-lg bg-surface accent-[hsl(var(--accent))]"
          />
          {children}
        </div>
      ) : children}
    </div>
  );
}
