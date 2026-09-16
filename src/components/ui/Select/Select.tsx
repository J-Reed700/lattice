import { type SelectHTMLAttributes, forwardRef } from 'react';

import { ChevronDown } from 'lucide-react';

/**
 * Select
 *
 * Purpose: Dropdown select control for choosing from predefined options
 *
 * Features:
 * - Native select element for reliability
 * - Custom styling that respects system behavior
 * - Label and helper text support
 * - Error state handling
 * - Dark mode support
 *
 * States: default, focus, error, disabled
 * Accessibility: WCAG AA, keyboard navigation, proper labels
 */

interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  error?: string;
  helperText?: string;
  options: SelectOption[];
}

const Select = forwardRef<HTMLSelectElement, SelectProps>(
  (
    {
      label,
      error,
      helperText,
      options,
      disabled,
      className = '',
      id,
      ...props
    },
    ref
  ) => {
    const selectId = id || `select-${Math.random().toString(36).substr(2, 9)}`;
    const errorId = error ? `${selectId}-error` : undefined;
    const helperId = helperText ? `${selectId}-helper` : undefined;
    const hasError = Boolean(error);

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={selectId}
            className="block text-sm font-medium text-[hsl(var(--text-secondary))] mb-1.5"
          >
            {label}
          </label>
        )}

        <div className="relative">
          <select
            ref={ref}
            id={selectId}
            disabled={disabled}
            aria-invalid={hasError}
            aria-describedby={errorId || helperId}
            className={`
              w-full px-4 py-2.5 pr-10
              bg-[hsl(var(--surface))]
              text-[hsl(var(--text-primary))]
              border rounded-md
              appearance-none
              transition-colors duration-fast
              focus:outline-none focus:ring-2 focus:ring-offset-0
              disabled:opacity-50 disabled:cursor-not-allowed
              ${
                hasError
                  ? 'border-[hsl(var(--danger-fg))] focus:ring-[hsl(var(--danger-fg))]'
                  : 'border-[hsl(var(--border-default))] focus:ring-[hsl(var(--ring))]'
              }
              ${className}
            `}
            {...props}
          >
            {options.map((option) => (
              <option
                key={option.value}
                value={option.value}
                disabled={option.disabled}
              >
                {option.label}
              </option>
            ))}
          </select>

          <div className="absolute right-3 top-1/2 -translate-y-1/2 pointer-events-none">
            <ChevronDown className="w-4 h-4 text-[hsl(var(--text-tertiary))]" strokeWidth={1.75} />
          </div>
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

        {helperText && !error && (
          <p
            id={helperId}
            className="mt-1.5 text-sm text-[hsl(var(--text-secondary))]"
          >
            {helperText}
          </p>
        )}
      </div>
    );
  }
);

Select.displayName = 'Select';

export default Select;
