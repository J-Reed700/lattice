import { type TextareaHTMLAttributes, forwardRef, useEffect, useId, useState } from 'react';

import { cn } from '@/lib/utils';

/**
 * Textarea
 *
 * First-class multi-line text input matching Lattice 2.0 `Input` tokens,
 * hairline borders, focus ring, and crisp architectural typography.
 */

export type TextareaResize = 'none' | 'vertical' | 'horizontal' | 'both';

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  error?: string;
  helperText?: string;
  showCharCount?: boolean;
  resize?: TextareaResize;
  validateOnBlur?: boolean;
  validate?: (value: string) => string | undefined;
  wrapperClassName?: string;
}

const resizeClasses: Record<TextareaResize, string> = {
  none: 'resize-none',
  vertical: 'resize-y',
  horizontal: 'resize-x',
  both: 'resize',
};

const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  (
    {
      label,
      error: externalError,
      helperText,
      showCharCount = false,
      resize = 'vertical',
      validateOnBlur = false,
      validate,
      disabled,
      className,
      wrapperClassName,
      id,
      maxLength,
      value,
      defaultValue,
      rows = 3,
      onBlur,
      onChange,
      ...props
    },
    ref
  ) => {
    const [internalError, setInternalError] = useState<string | undefined>();
    const [charCount, setCharCount] = useState(() => String(value ?? defaultValue ?? '').length);
    const generatedId = useId();

    const error = externalError || internalError;
    const textareaId = id ?? `textarea-${generatedId}`;
    const errorId = error ? `${textareaId}-error` : undefined;
    const helperId = helperText ? `${textareaId}-helper` : undefined;
    const hasError = Boolean(error);

    useEffect(() => {
      if (value !== undefined) setCharCount(String(value ?? '').length);
    }, [value]);

    const handleBlur = (e: React.FocusEvent<HTMLTextAreaElement>) => {
      if (validateOnBlur && validate) {
        setInternalError(validate(e.target.value));
      }
      onBlur?.(e);
    };

    const showFooter = Boolean((helperText && !error) || (showCharCount && maxLength));

    return (
      <div className={cn('w-full', wrapperClassName)}>
        {label && (
          <label
            htmlFor={textareaId}
            className="mb-1.5 block text-xs font-medium text-text-secondary"
          >
            {label}
          </label>
        )}

        <textarea
          ref={ref}
          id={textareaId}
          rows={rows}
          disabled={disabled}
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
            'min-h-[80px] w-full rounded-lg border bg-surface px-3 py-2 text-ui leading-relaxed text-text-primary shadow-control transition-[border-color,box-shadow,background-color] duration-fast',
            'placeholder:text-text-muted',
            'focus:outline-hidden focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-offset-0',
            'disabled:cursor-not-allowed disabled:opacity-50',
            resizeClasses[resize],
            hasError
              ? 'border-danger-fg focus-visible:border-danger-fg focus-visible:ring-danger-fg'
              : 'border-border-default hover:border-border-strong focus-visible:border-accent/60 focus-visible:ring-ring disabled:hover:border-border-default',
            className,
          )}
          {...props}
        />

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
              <p id={helperId} className="text-xs text-text-secondary">
                {helperText}
              </p>
            )}
            {showCharCount && maxLength && (
              <p
                className={cn(
                  'ml-auto text-xs tabular-nums',
                  charCount >= maxLength
                    ? 'text-danger-fg'
                    : charCount > maxLength * 0.9
                      ? 'text-warning-fg'
                      : 'text-text-secondary',
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

Textarea.displayName = 'Textarea';

export { Textarea };
export default Textarea;
