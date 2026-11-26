/**
 * Shared minimalist Settings control wrappers built on Wave 1 `@/components/ui`
 * primitives (`Select`, `Slider`, `NumberStepper`).
 */

import { useEffect, useRef } from 'react';

import {
  NumberStepper,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Slider,
} from '@/components/ui';
import { cn } from '@/lib/utils';

const EMPTY_SENTINEL = '__lattice_select_empty__';

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SettingsSelectProps {
  id?: string;
  value: string;
  onValueChange: (value: string) => void;
  options: SelectOption[];
  placeholder?: string;
  disabled?: boolean;
  'aria-label'?: string;
  className?: string;
}

/**
 * Radix Select configured for Settings rows (`h-8`, hairline border, clean
 * empty-value mapping since Radix Select items cannot carry an empty string).
 */
export function SettingsSelect({
  id,
  value,
  onValueChange,
  options,
  placeholder,
  disabled,
  'aria-label': ariaLabel,
  className,
}: SettingsSelectProps) {
  const encodedValue = value === '' ? EMPTY_SENTINEL : value;
  const hasMatchingOption = options.some((option) => option.value === value);

  return (
    <Select
      value={hasMatchingOption ? encodedValue : undefined}
      onValueChange={(next) => onValueChange(next === EMPTY_SENTINEL ? '' : next)}
      disabled={disabled}
    >
      <SelectTrigger
        id={id}
        aria-label={ariaLabel}
        className={cn('w-full', className)}
      >
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem
            key={option.value === '' ? EMPTY_SENTINEL : option.value}
            value={option.value === '' ? EMPTY_SENTINEL : option.value}
            disabled={option.disabled}
          >
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export interface SettingsNumberFieldProps {
  id?: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  integer?: boolean;
  disabled?: boolean;
  prefix?: string;
  suffix?: string;
  'aria-label'?: string;
  className?: string;
  inputClassName?: string;
  /** Called live while editing (typing or stepping) so parent drafts stay in sync. */
  onDraftChange?: (value: number) => void;
  /** Called once when the user commits (blur, Enter, step button, or arrow key). */
  onCommit: (value: number) => void;
}

function normalizeCommit(raw: number, min?: number, max?: number, integer?: boolean): number {
  let next = raw;
  if (min !== undefined && next < min) next = min;
  if (max !== undefined && next > max) next = max;
  if (integer) next = Math.round(next);
  return Object.is(next, -0) ? 0 : next;
}

/**
 * Precision `NumberStepper` tailored for persisted Settings fields:
 * - Typing updates draft state without spamming backend saves.
 * - Blur, Enter, +/- buttons, and Arrow/Home/End keys commit cleanly once.
 */
export function SettingsNumberField({
  id,
  value,
  min,
  max,
  step = 1,
  integer = false,
  disabled = false,
  prefix,
  suffix,
  'aria-label': ariaLabel,
  className,
  inputClassName,
  onDraftChange,
  onCommit,
}: SettingsNumberFieldProps) {
  const latestRef = useRef<number>(value);
  const isTypingRef = useRef(false);
  const dirtyRef = useRef(false);
  const rawInputRef = useRef<string | null>(null);

  useEffect(() => {
    latestRef.current = value;
  }, [value]);

  const commitValue = (candidate: number) => {
    const normalized = normalizeCommit(candidate, min, max, integer);
    latestRef.current = normalized;
    dirtyRef.current = false;
    rawInputRef.current = null;
    onDraftChange?.(normalized);
    onCommit(normalized);
  };

  const clearTypingState = () => {
    isTypingRef.current = false;
    dirtyRef.current = false;
    rawInputRef.current = null;
  };

  return (
    <div
      className={cn('inline-flex', className)}
      onMouseDownCapture={(event) => {
        if (event.target instanceof Element && event.target.closest('button')) {
          clearTypingState();
        }
      }}
      onClickCapture={(event) => {
        if (event.target instanceof Element && event.target.closest('button')) {
          clearTypingState();
        }
      }}
      onChangeCapture={(event) => {
        if (event.target instanceof HTMLInputElement) {
          isTypingRef.current = true;
          dirtyRef.current = true;
          rawInputRef.current = event.target.value;
        }
      }}
      onKeyDownCapture={(event) => {
        if (
          event.key === 'ArrowUp' ||
          event.key === 'ArrowDown' ||
          event.key === 'PageUp' ||
          event.key === 'PageDown' ||
          event.key === 'Home' ||
          event.key === 'End'
        ) {
          clearTypingState();
        }
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          isTypingRef.current = false;
          const parsed = rawInputRef.current !== null && rawInputRef.current.trim() !== ''
            ? Number(rawInputRef.current.trim())
            : Number.NaN;
          commitValue(Number.isFinite(parsed) ? parsed : latestRef.current);
        }
      }}
      onBlur={() => {
        isTypingRef.current = false;
        if (dirtyRef.current || rawInputRef.current !== null) {
          const parsed = rawInputRef.current !== null && rawInputRef.current.trim() !== ''
            ? Number(rawInputRef.current.trim())
            : Number.NaN;
          commitValue(Number.isFinite(parsed) ? parsed : latestRef.current);
        } else {
          // Even an untouched blur notifies callers that rely on blur-to-settle
          // (callers compare against stored values to avoid no-op writes).
          onCommit(normalizeCommit(latestRef.current, min, max, integer));
        }
      }}
    >
      <NumberStepper
        id={id}
        value={value}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        prefix={prefix}
        suffix={suffix}
        aria-label={ariaLabel}
        inputClassName={inputClassName}
        onValueChange={(next) => {
          const normalized = integer ? Math.round(next) : next;
          latestRef.current = normalized;
          onDraftChange?.(normalized);
          if (!isTypingRef.current) {
            dirtyRef.current = false;
            rawInputRef.current = null;
            onCommit(normalized);
          }
        }}
      />
    </div>
  );
}

export interface RangeStepperControlProps {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  /** Optional slider upper bound when the stepper allows values above the slider track. */
  sliderMax?: number;
  step: number;
  integer?: boolean;
  disabled?: boolean;
  suffix?: string;
  minLabel?: string;
  maxLabel?: string;
  formatValue?: (value: number) => string;
  onDraftChange?: (value: number) => void;
  onCommit: (value: number) => void;
}

/**
 * Paired `Slider` + `NumberStepper` control for bounded numeric settings.
 * Dragging the slider updates the live tabular-nums stepper smoothly and
 * commits once on release; clicking +/- or blurring commits immediately.
 */
export function RangeStepperControl({
  id,
  label,
  value,
  min,
  max,
  sliderMax,
  step,
  integer = false,
  disabled = false,
  suffix,
  minLabel,
  maxLabel,
  formatValue,
  onDraftChange,
  onCommit,
}: RangeStepperControlProps) {
  const sliderUpper = sliderMax ?? max;
  const latestSliderRef = useRef(value);
  const sliderMovedRef = useRef(false);

  useEffect(() => {
    latestSliderRef.current = value;
  }, [value]);

  const commitSliderIfMoved = () => {
    if (!sliderMovedRef.current) return;
    sliderMovedRef.current = false;
    onCommit(normalizeCommit(latestSliderRef.current, min, max, integer));
  };

  return (
    <div className="flex w-full items-center gap-3">
      <div className="min-w-[7rem] flex-1">
        <Slider
          value={Math.min(Math.max(value, min), sliderUpper)}
          min={min}
          max={sliderUpper}
          step={step}
          disabled={disabled}
          minLabel={minLabel}
          maxLabel={maxLabel}
          formatValue={formatValue}
          aria-label={`${label} slider`}
          onValueChange={(next) => {
            const normalized = integer ? Math.round(next) : next;
            latestSliderRef.current = normalized;
            sliderMovedRef.current = true;
            onDraftChange?.(normalized);
          }}
          onPointerUp={commitSliderIfMoved}
          onTouchEnd={commitSliderIfMoved}
          onKeyUp={commitSliderIfMoved}
          onBlur={commitSliderIfMoved}
        />
      </div>
      <SettingsNumberField
        id={id}
        value={value}
        min={min}
        max={sliderMax !== undefined ? undefined : max}
        step={step}
        integer={integer}
        disabled={disabled}
        suffix={suffix}
        inputClassName="w-14"
        onDraftChange={onDraftChange}
        onCommit={onCommit}
      />
    </div>
  );
}
