import { type InputHTMLAttributes, forwardRef, useId, useState } from 'react';

import { cn } from '@/lib/utils';

/**
 * Slider
 *
 * Accessible minimalist range slider backed by a native `<input type="range">`
 * so keyboard navigation, assistive technology, and pointer dragging work
 * consistently across platforms. Uses copper `--accent` for the active track
 * and tactile thumb border.
 */

export interface SliderProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type' | 'value' | 'defaultValue' | 'min' | 'max' | 'step'> {
  value?: number;
  defaultValue?: number;
  min?: number;
  max?: number;
  step?: number;
  onValueChange?: (value: number) => void;
  label?: string;
  description?: string;
  /** Show the formatted current value inline beside the label (or above track). */
  showValue?: boolean;
  /** Format the displayed value (and `aria-valuetext` when `aria-valuetext` isn't explicitly passed). */
  formatValue?: (value: number) => string;
  /** Optional subtle caption below the left end of the track. */
  minLabel?: string;
  /** Optional subtle caption below the right end of the track. */
  maxLabel?: string;
  wrapperClassName?: string;
}

function clamp(n: number, min: number, max: number): number {
  return Math.min(Math.max(n, min), max);
}

const Slider = forwardRef<HTMLInputElement, SliderProps>(
  (
    {
      value,
      defaultValue,
      min = 0,
      max = 100,
      step = 1,
      onValueChange,
      onChange,
      label,
      description,
      showValue = false,
      formatValue,
      minLabel,
      maxLabel,
      disabled = false,
      className,
      wrapperClassName,
      id,
      'aria-valuetext': ariaValueText,
      ...props
    },
    ref
  ) => {
    const generatedId = useId();
    const sliderId = id ?? `slider-${generatedId}`;
    const descriptionId = description ? `${sliderId}-description` : undefined;

    const isControlled = value !== undefined;
    const [uncontrolledValue, setUncontrolledValue] = useState<number>(() =>
      clamp(defaultValue ?? min, min, max),
    );

    const currentValue = isControlled ? value : uncontrolledValue;
    const safeValue = Number.isFinite(currentValue) ? clamp(currentValue, min, max) : min;
    const percent = max > min ? clamp(((safeValue - min) / (max - min)) * 100, 0, 100) : 0;
    const formattedValue = formatValue ? formatValue(safeValue) : String(safeValue);
    // Keep the 16px thumb center bounded within [8px, calc(100% - 8px)] so the
    // thumb never clips outside overflow-hidden containers at 0% or 100%.
    const thumbOffset = Number((8 - (percent / 100) * 16).toFixed(4));
    const thumbLeft =
      thumbOffset === 0
        ? `${percent}%`
        : thumbOffset > 0
          ? `calc(${percent}% + ${thumbOffset}px)`
          : `calc(${percent}% - ${Math.abs(thumbOffset)}px)`;
    const fillWidth = percent === 0 ? '0%' : percent === 100 ? '100%' : thumbLeft;

    const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
      const next = Number(event.currentTarget.value);
      if (!Number.isNaN(next)) {
        if (!isControlled) {
          setUncontrolledValue(next);
        }
        onValueChange?.(next);
      }
      onChange?.(event);
    };

    const showHeader = Boolean(label || showValue);

    return (
      <div className={cn('w-full', wrapperClassName)}>
        {showHeader && (
          <div className="mb-1.5 flex items-baseline justify-between gap-2">
            {label ? (
              <label
                htmlFor={sliderId}
                className={cn(
                  'text-xs font-medium text-text-secondary',
                  disabled && 'opacity-50',
                )}
              >
                {label}
              </label>
            ) : (
              <span />
            )}
            {showValue && (
              <span
                className={cn(
                  'text-xs font-medium tabular-nums text-text-primary',
                  disabled && 'opacity-50',
                )}
              >
                {formattedValue}
              </span>
            )}
          </div>
        )}

        <div
          className={cn(
            'group relative flex h-6 w-full items-center select-none',
            disabled && 'opacity-50',
            className,
          )}
        >
          {/* Native range input handles keyboard, pointer, and screen reader semantics */}
          <input
            ref={ref}
            id={sliderId}
            type="range"
            min={min}
            max={max}
            step={step}
            value={isControlled ? safeValue : undefined}
            defaultValue={isControlled ? undefined : safeValue}
            disabled={disabled}
            aria-describedby={descriptionId}
            aria-valuetext={ariaValueText ?? (formatValue ? formattedValue : undefined)}
            onChange={handleChange}
            className={cn(
              'peer absolute inset-0 z-10 h-full w-full opacity-0',
              disabled ? 'cursor-not-allowed' : 'cursor-pointer',
            )}
            {...props}
          />

          {/* Track background */}
          <div
            aria-hidden="true"
            className="pointer-events-none relative h-1.5 w-full overflow-hidden rounded-full bg-[hsl(var(--text-primary)/0.12)]"
          >
            {/* Active copper progress fill */}
            <div
              data-slot="slider-fill"
              className="h-full rounded-full bg-accent transition-[width] duration-instant ease-out"
              style={{ width: fillWidth }}
            />
          </div>

          {/* Tactile thumb */}
          <div
            aria-hidden="true"
            data-slot="slider-thumb"
            className={cn(
              'pointer-events-none absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-accent bg-surface-overlay shadow-sm transition-[transform,box-shadow,border-color] duration-fast ease-out',
              !disabled && 'group-hover:scale-110 peer-active:scale-95',
              'peer-focus-visible:ring-2 peer-focus-visible:ring-ring peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-bg',
            )}
            style={{ left: thumbLeft }}
          />
        </div>

        {(minLabel || maxLabel) && (
          <div className="mt-0.5 flex items-center justify-between text-[11px] text-text-muted tabular-nums">
            <span>{minLabel}</span>
            <span>{maxLabel}</span>
          </div>
        )}

        {description && (
          <p id={descriptionId} className="mt-1 text-xs text-text-secondary">
            {description}
          </p>
        )}
      </div>
    );
  }
);

Slider.displayName = 'Slider';

export { Slider };
export default Slider;
