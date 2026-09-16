import { type InputHTMLAttributes, forwardRef, useState, useEffect } from 'react';

/**
 * Input
 *
 * Purpose: Text input field for user data entry
 *
 * Features:
 * - Icon support (left and right)
 * - Error state with message
 * - Helper text
 * - Loading state
 * - Dark mode support
 * - Character counter
 * - Inline validation
 * - Clear button
 *
 * States: default, focus, error, disabled, loading
 * Accessibility: WCAG AA, proper labels, error announcements
 */

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
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
}

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
      className = '',
      id,
      showCharCount = false,
      showClearButton = false,
      onClear,
      maxLength,
      value,
      validateOnBlur = false,
      validate,
      onBlur,
      ...props
    },
    ref
  ) => {
    const [internalError, setInternalError] = useState<string | undefined>();
    const [charCount, setCharCount] = useState(0);

    const error = externalError || internalError;

    const inputId = id || `input-${Math.random().toString(36).substr(2, 9)}`;
    const errorId = error ? `${inputId}-error` : undefined;
    const helperId = helperText ? `${inputId}-helper` : undefined;
    const hasError = Boolean(error);

    useEffect(() => {
      if (value) {
        setCharCount(String(value).length);
      } else {
        setCharCount(0);
      }
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

    // Show right icon, loading spinner, or clear button
    const showRightContent = rightIcon || isLoading || (showClearButton && charCount > 0);

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={inputId}
            className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-1.5"
          >
            {label}
          </label>
        )}

        <div className="relative">
          {leftIcon && (
            <div className="absolute left-3 top-1/2 -translate-y-1/2 text-[hsl(var(--text-tertiary))] pointer-events-none">
              {leftIcon}
            </div>
          )}

          <input
            ref={ref}
            id={inputId}
            disabled={disabled || isLoading}
            aria-invalid={hasError}
            aria-describedby={errorId || helperId}
            maxLength={maxLength}
            value={value}
            onBlur={handleBlur}
            className={`
              w-full px-4 py-2.5
              bg-[hsl(var(--surface))]
              text-[hsl(var(--text-primary))]
              border rounded-md
              transition-colors duration-fast
              placeholder-[hsl(var(--text-tertiary))]
              focus:outline-none focus:ring-2 focus:ring-offset-0
              disabled:opacity-50 disabled:cursor-not-allowed
              ${leftIcon ? 'pl-10' : ''}
              ${showRightContent ? 'pr-10' : ''}
              ${hasError
                ? 'border-[hsl(var(--danger-fg))] focus:ring-[hsl(var(--danger-fg))]'
                : 'border-[hsl(var(--border-default))] focus:ring-[hsl(var(--ring))]'
              }
              ${className}
            `}
            {...props}
          />

          {showRightContent && (
            <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center gap-1">
              {isLoading ? (
                <svg
                  className="animate-spin h-4 w-4 text-[hsl(var(--text-tertiary))]"
                  xmlns="http://www.w3.org/2000/svg"
                  fill="none"
                  viewBox="0 0 24 24"
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
                  className="text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
                  aria-label="Clear input"
                  tabIndex={-1}
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.75} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              ) : rightIcon ? (
                <div className="text-[hsl(var(--text-tertiary))]">{rightIcon}</div>
              ) : null}
            </div>
          )}
        </div>

        {error && (
          <p
            id={errorId}
            className="mt-1.5 text-sm text-[hsl(var(--danger-fg))] flex items-start gap-1"
            role="alert"
          >
            <svg
              className="w-4 h-4 mt-0.5 flex-shrink-0"
              fill="currentColor"
              viewBox="0 0 20 20"
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

        <div className="flex items-center justify-between mt-1.5">
          {helperText && !error && (
            <p
              id={helperId}
              className="text-sm text-[hsl(var(--text-secondary))]"
            >
              {helperText}
            </p>
          )}
          {showCharCount && maxLength && (
            <p
              className={`text-xs ${
                charCount > maxLength * 0.9
                  ? 'text-[hsl(var(--warning-fg))]'
                  : charCount === maxLength
                  ? 'text-[hsl(var(--danger-fg))]'
                  : 'text-[hsl(var(--text-secondary))]'
              }`}
            >
              {charCount} / {maxLength}
            </p>
          )}
        </div>
      </div>
    );
  }
);

Input.displayName = 'Input';

export default Input;
