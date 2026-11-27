import { forwardRef, useId, useState } from 'react';

import { Minus, Plus } from 'lucide-react';

import { cn } from '@/lib/utils';

/**
 * NumberStepper
 *
 * Precision minimalist numeric control with decrement/increment buttons,
 * tabular-nums display, floating-point step rounding, min/max clamping,
 * and full spinbutton keyboard navigation (ArrowUp/Down, PageUp/Down, Home/End).
 */

export type NumberStepperSize = 'sm' | 'md';

export interface NumberStepperProps {
  value?: number;
  defaultValue?: number;
  onValueChange?: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  readOnly?: boolean;
  id?: string;
  name?: string;
  label?: string;
  helperText?: string;
  error?: string;
  prefix?: string;
  suffix?: string;
  /** Visual control height: `sm` (28px), `md` (32px, default). */
  size?: NumberStepperSize;
  decrementLabel?: string;
  incrementLabel?: string;
  'aria-label'?: string;
  'aria-labelledby'?: string;
  className?: string;
  inputClassName?: string;
}

function stepPrecision(step: number): number {
  if (!Number.isFinite(step) || step === 0) return 0;
  const str = String(step);
  if (str.includes('e-')) {
    const [base, exp] = str.split('e-');
    const decimals = (base.split('.')[1] ?? '').length;
    return decimals + Number(exp);
  }
  return (str.split('.')[1] ?? '').length;
}

function clampNumber(n: number, min?: number, max?: number): number {
  let clamped = n;
  if (min !== undefined && clamped < min) clamped = min;
  if (max !== undefined && clamped > max) clamped = max;
  return Object.is(clamped, -0) ? 0 : clamped;
}

function roundToStep(n: number, step: number): number {
  const precision = stepPrecision(step);
  const rounded = precision > 0 ? Number(n.toFixed(precision)) : Math.round(n);
  return Object.is(rounded, -0) ? 0 : rounded;
}

const NumberStepper = forwardRef<HTMLInputElement, NumberStepperProps>(
  (
    {
      value,
      defaultValue,
      onValueChange,
      min,
      max,
      step = 1,
      disabled = false,
      readOnly = false,
      id,
      name,
      label,
      helperText,
      error,
      prefix,
      suffix,
      size = 'md',
      decrementLabel = 'Decrease value',
      incrementLabel = 'Increase value',
      'aria-label': ariaLabel,
      'aria-labelledby': ariaLabelledBy,
      className,
      inputClassName,
    },
    ref
  ) => {
    const generatedId = useId();
    const inputId = id ?? `number-stepper-${generatedId}`;
    const errorId = error ? `${inputId}-error` : undefined;
    const helperId = helperText ? `${inputId}-helper` : undefined;
    const hasError = Boolean(error);

    const isControlled = value !== undefined;
    const initialNumeric = clampNumber(value ?? defaultValue ?? min ?? 0, min, max);

    const [uncontrolledValue, setUncontrolledValue] = useState<number>(initialNumeric);
    const currentNumeric = isControlled ? value : uncontrolledValue;
    // `editingDraft` is only non-null while the user is actively typing raw text.
    // On step/Enter/blur it resets to `null` so controlled components always stay
    // locked to `value` even if the parent rejects or normalizes `onValueChange`.
    const [editingDraft, setEditingDraft] = useState<string | null>(null);
    const displayValue = editingDraft ?? String(currentNumeric);

    const parsedDraft =
      editingDraft !== null && editingDraft.trim() !== ''
        ? Number(editingDraft.trim())
        : Number.NaN;
    const effectiveNumeric = Number.isFinite(parsedDraft)
      ? clampNumber(parsedDraft, min, max)
      : Number.isFinite(currentNumeric)
        ? currentNumeric
        : (min ?? 0);

    const commit = (raw: number, snapPrecision = false) => {
      const candidate = snapPrecision ? roundToStep(raw, step) : raw;
      const next = clampNumber(candidate, min, max);
      setEditingDraft(null);
      if (!isControlled) {
        setUncontrolledValue(next);
      }
      if (next !== currentNumeric) {
        onValueChange?.(next);
      }
    };

    const stepBy = (delta: number) => {
      if (disabled || readOnly) return;
      commit(effectiveNumeric + delta, true);
    };

    const handleInputChange = (event: React.ChangeEvent<HTMLInputElement>) => {
      const nextText = event.currentTarget.value;
      setEditingDraft(nextText);
      const trimmed = nextText.trim();
      if (trimmed === '' || trimmed === '-' || trimmed === '.' || trimmed === '-.') {
        return;
      }
      const parsed = Number(trimmed);
      if (Number.isFinite(parsed)) {
        const withinMin = min === undefined || parsed >= min;
        const withinMax = max === undefined || parsed <= max;
        if (withinMin && withinMax) {
          if (!isControlled) {
            setUncontrolledValue(parsed);
          }
          if (parsed !== currentNumeric) {
            onValueChange?.(parsed);
          }
        }
      }
    };

    const commitDraft = () => {
      if (editingDraft === null) return;
      const trimmed = editingDraft.trim();
      const parsed = Number(trimmed);
      if (trimmed === '' || !Number.isFinite(parsed)) {
        setEditingDraft(null);
        return;
      }
      commit(parsed);
    };

    const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (disabled || readOnly) return;
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        stepBy(step);
      } else if (event.key === 'ArrowDown') {
        event.preventDefault();
        stepBy(-step);
      } else if (event.key === 'PageUp') {
        event.preventDefault();
        stepBy(step * 10);
      } else if (event.key === 'PageDown') {
        event.preventDefault();
        stepBy(-step * 10);
      } else if (event.key === 'Home' && min !== undefined) {
        event.preventDefault();
        commit(min);
      } else if (event.key === 'End' && max !== undefined) {
        event.preventDefault();
        commit(max);
      } else if (event.key === 'Enter') {
        commitDraft();
      }
    };

    const atMin = min !== undefined && effectiveNumeric <= min;
    const atMax = max !== undefined && effectiveNumeric >= max;
    const heightClass = size === 'sm' ? 'h-7 rounded-md' : 'h-8 rounded-lg';
    const buttonWidthClass = size === 'sm' ? 'w-7' : 'w-8';

    return (
      <div className={cn('inline-flex flex-col', className)}>
        {label && (
          <label
            htmlFor={inputId}
            className="mb-1.5 block text-xs font-medium text-text-secondary"
          >
            {label}
          </label>
        )}

        <div
          className={cn(
            'inline-flex items-stretch overflow-hidden border bg-surface shadow-control transition-[border-color,box-shadow] duration-fast',
            heightClass,
            hasError
              ? 'border-danger-fg focus-within:border-danger-fg focus-within:ring-2 focus-within:ring-danger-fg'
              : 'border-border-default hover:border-border-strong focus-within:border-accent/60 focus-within:ring-2 focus-within:ring-ring',
            disabled && 'cursor-not-allowed opacity-50 hover:border-border-default',
          )}
        >
          <button
            type="button"
            tabIndex={-1}
            aria-label={decrementLabel}
            disabled={disabled || readOnly || atMin}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => stepBy(-step)}
            className={cn(
              'inline-flex shrink-0 items-center justify-center border-r border-border-subtle text-text-secondary transition-colors duration-fast',
              'hover:bg-[hsl(var(--text-primary)/0.05)] hover:text-text-primary active:bg-[hsl(var(--text-primary)/0.09)]',
              'disabled:pointer-events-none disabled:opacity-35',
              buttonWidthClass,
            )}
          >
            <Minus className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          </button>

          <div className="flex min-w-0 flex-1 items-center justify-center gap-1 px-2">
            {prefix && (
              <span className="select-none text-xs text-text-muted">{prefix}</span>
            )}
            <input
              ref={ref}
              id={inputId}
              name={name}
              type="text"
              inputMode="decimal"
              role="spinbutton"
              value={displayValue}
              disabled={disabled}
              readOnly={readOnly}
              aria-label={ariaLabel}
              aria-labelledby={ariaLabelledBy}
              aria-valuemin={min}
              aria-valuemax={max}
              aria-valuenow={Number.isFinite(currentNumeric) ? currentNumeric : undefined}
              aria-invalid={hasError}
              aria-describedby={errorId || helperId}
              onBlur={commitDraft}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              className={cn(
                'w-14 min-w-0 bg-transparent text-center text-ui font-medium tabular-nums text-text-primary outline-hidden focus-visible:outline-hidden disabled:cursor-not-allowed',
                inputClassName,
              )}
            />
            {suffix && (
              <span className="select-none text-xs text-text-muted">{suffix}</span>
            )}
          </div>

          <button
            type="button"
            tabIndex={-1}
            aria-label={incrementLabel}
            disabled={disabled || readOnly || atMax}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => stepBy(step)}
            className={cn(
              'inline-flex shrink-0 items-center justify-center border-l border-border-subtle text-text-secondary transition-colors duration-fast',
              'hover:bg-[hsl(var(--text-primary)/0.05)] hover:text-text-primary active:bg-[hsl(var(--text-primary)/0.09)]',
              'disabled:pointer-events-none disabled:opacity-35',
              buttonWidthClass,
            )}
          >
            <Plus className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>

        {error && (
          <p id={errorId} role="alert" className="mt-1.5 text-xs text-danger-fg">
            {error}
          </p>
        )}

        {helperText && !error && (
          <p id={helperId} className="mt-1.5 text-xs text-text-secondary">
            {helperText}
          </p>
        )}
      </div>
    );
  }
);

NumberStepper.displayName = 'NumberStepper';

export { NumberStepper };
export default NumberStepper;
