import { type InputHTMLAttributes, forwardRef, useState, useEffect, useId } from 'react';

import { cn } from '@/lib/utils';

/**
 * Input
 *
 * Compact desktop text input field matching the Lattice 2.0 minimalist system.
 * Default density is `md` (`h-8` / 32px, `rounded-lg`), with optional `sm` (28px)
 * and `lg` (40px) sizes.
 */

export type InputSize = 'sm' | 'md' | 'lg';

export interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'size'> {
  label?: string;
  error?: string;
  helperText?: string;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
  isLoading?: boolean;
  showCharCount?: boolean;
  showClearButton?: boolean;
  onClear?: () => void;
  validateOnBlur?: boolean;
  validate?: (value: string) => string | undefined;
  /** Visual control height: `sm` (28px), `md` (32px, default), `lg` (40px). */
  size?: InputSize;
  /** Native HTML `size` attribute if needed. */
  htmlSize?: number;
}

const sizeClasses: Record<InputSize, string> = {
  sm: 'h-7 rounded-md px-2.5 text-xs',
  md: 'h-8 rounded-lg px-2.5 text-ui',
  lg: 'h-10 rounded-lg px-3.5 text-sm',
};

const Input = forwardRef<HTMLInputElement, InputProps>(
  (
    {
      label,
      error: externalError,
      helperText,
      leftIcon,
      rightIcon,
      isLoading,
      disabled,
      className,
      id,
      showCharCount = false,
      showClearButton = false,
      onClear,
      maxLength,
      value,
      defaultValue,
      validateOnBlur = false,
      validate,
      onBlur,
      onChange,
      size = 'md',
      htmlSize,
      ...props
    },
    ref
  ) => {
    const [internalError, setInternalError] = useState<string | undefined>();
    const [charCount, setCharCount] = useState(() => String(value ?? defaultValue ?? '').length);
    const generatedId = useId();

    const error = externalError || internalError;

    const inputId = id ?? `input-${generatedId}`;
    const errorId = error ? `${inputId}-error` : undefined;
    const helperId = helperText ? `${inputId}-helper` : undefined;
    const hasError = Boolean(error);

    useEffect(() => {
      if (value !== undefined) setCharCount(String(value ?? '').length);
    }, [value]);

    const handleBlur = (e: React.FocusEvent<HTMLInputElement>) => {
      if (validateOnBlur && validate) {
        const validationError = validate(e.target.value);
        setInternalError(validationError);
      }
      onBlur?.(e);
    };

    const handleClear = () => {
      setInternalError(undefined);
      setCharCount(0);
      onClear?.();
    };

    const showRightContent = Boolean(rightIcon || isLoading || (showClearButton && charCount > 0));
    const showFooter = Boolean((helperText && !error) || (showCharCount && maxLength));

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={inputId}
            className="mb-1.5 block text-xs font-medium text-text-secondary"
          >
            {label}
          </label>
        )}

        <div className="relative">
          {leftIcon && (
            <div className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary">
              {leftIcon}
            </div>
          )}

          <input
            ref={ref}
            id={inputId}
            size={htmlSize}
            disabled={disabled || isLoading}
            aria-invalid={hasError}
            aria-describedby={errorId || helperId}
            maxLength={maxLength}
            value={value}
            defaultValue={defaultValue}
            onChange={(event) => {
              setCharCount(event.currentTarget.value.length);
              onChange?.(event);
            }}
            onBlur={handleBlur}
            className={cn(
              'w-full border bg-surface text-text-primary shadow-control transition-[border-color,box-shadow,background-color] duration-fast',
              'placeholder:text-text-muted',
              'focus:outline-hidden focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-offset-0',
              'disabled:cursor-not-allowed disabled:opacity-50',
              sizeClasses[size],
              leftIcon && 'pl-10',
              showRightContent && 'pr-9',
              hasError
                ? 'border-danger-fg focus-visible:border-danger-fg focus-visible:ring-danger-fg'
                : 'border-border-default hover:border-border-strong focus-visible:border-accent/60 focus-visible:ring-ring disabled:hover:border-border-default',
              className,
            )}
            {...props}
          />

          {showRightContent && (
            <div className="absolute right-2.5 top-1/2 flex -translate-y-1/2 items-center gap-1">
              {isLoading ? (
                <svg
                  className="h-3.5 w-3.5 animate-spin text-text-tertiary"
                  xmlns="http://www.w3.org/2000/svg"
                  fill="none"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <circle
                    className="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    strokeWidth="4"
                  />
                  <path
                    className="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                  />
                </svg>
              ) : showClearButton && charCount > 0 ? (
                <button
                  type="button"
                  onClick={handleClear}
                  className="rounded-sm p-0.5 text-text-tertiary transition-colors duration-fast hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  aria-label="Clear input"
                  tabIndex={-1}
                >
                  <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.75} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              ) : rightIcon ? (
                <div className="text-text-tertiary">{rightIcon}</div>
              ) : null}
            </div>
          )}
        </div>

        {error && (
          <p
            id={errorId}
            className="mt-1.5 flex items-start gap-1 text-xs text-danger-fg"
            role="alert"
          >
            <svg
              className="mt-0.5 h-3.5 w-3.5 shrink-0"
              fill="currentColor"
              viewBox="0 0 20 20"
              aria-hidden="true"
            >
              <path
                fillRule="evenodd"
                d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                clipRule="evenodd"
              />
            </svg>
            <span>{error}</span>
          </p>
        )}

        {showFooter && (
          <div className="mt-1.5 flex items-center justify-between gap-2">
            {helperText && !error && (
              <p
                id={helperId}
                className="text-xs text-text-secondary"
              >
                {helperText}
              </p>
            )}
            {showCharCount && maxLength && (
              <p
                className={cn(
                  'ml-auto text-xs tabular-nums',
                  charCount >= maxLength
                    ? 'text-[hsl(var(--danger-fg))]'
                    : charCount > maxLength * 0.9
                      ? 'text-[hsl(var(--warning-fg))]'
                      : 'text-[hsl(var(--text-secondary))]',
                )}
              >
                {charCount} / {maxLength}
              </p>
            )}
          </div>
        )}
      </div>
    );
  }
);

Input.displayName = 'Input';

export { Input };
export default Input;
