import { forwardRef, type InputHTMLAttributes } from 'react';

import { Check } from 'lucide-react';

/**
 * Checkbox
 *
 * Purpose: Multi-select control for toggling individual options
 *
 * Features:
 * - Custom styled checkbox with smooth transitions
 * - Label and description support
 * - Indeterminate state support
 * - Error state handling
 * - Dark mode support
 *
 * States: default, hover, focus, checked, indeterminate, disabled
 * Accessibility: WCAG AA, keyboard navigation, proper labels
 */

interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label?: string;
  description?: string;
  error?: string;
  indeterminate?: boolean;
  onCheckedChange?: (checked: boolean) => void;
}

const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(
  (
    {
      label,
      description,
      error,
      indeterminate = false,
      checked,
      onCheckedChange,
      disabled,
      className = '',
      id,
      ...props
    },
    ref
  ) => {
    const checkboxId = id || `checkbox-${Math.random().toString(36).substr(2, 9)}`;
    const errorId = error ? `${checkboxId}-error` : undefined;
    const descriptionId = description ? `${checkboxId}-description` : undefined;
    const hasError = Boolean(error);

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      onCheckedChange?.(e.target.checked);
      props.onChange?.(e);
    };

    return (
      <div className={className}>
        <div className="flex items-start gap-3">
          <div className="flex items-center h-6">
            <div className="relative">
              <input
                ref={ref}
                id={checkboxId}
                type="checkbox"
                checked={checked}
                onChange={handleChange}
                disabled={disabled}
                aria-invalid={hasError}
                aria-describedby={errorId || descriptionId}
                className="sr-only peer"
                {...props}
              />
              <label
                htmlFor={checkboxId}
                className={`
                  flex items-center justify-center
                  w-5 h-5 rounded border-2
                  transition-colors duration-fast
                  cursor-pointer
                  peer-focus-visible:ring-2 peer-focus-visible:ring-[hsl(var(--ring))] peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-[hsl(var(--bg))]
                  ${
                    hasError
                      ? 'border-[hsl(var(--danger-fg))]'
                      : 'border-[hsl(var(--border-default))]'
                  }
                  ${
                    checked || indeterminate
                      ? 'bg-[hsl(var(--accent))] border-[hsl(var(--accent))] '
                      : 'bg-[hsl(var(--surface))]'
                  }
                  ${
                    disabled
                      ? 'opacity-50 cursor-not-allowed'
                      : 'hover:border-[hsl(var(--accent))]'
                  }
                `}
              >
                {checked && !indeterminate && (
                  <Check className="w-3.5 h-3.5 text-[hsl(var(--accent-fg))]" strokeWidth={3} />
                )}
                {indeterminate && (
                  <div className="w-2.5 h-0.5 bg-[hsl(var(--accent-fg))] rounded-full" />
                )}
              </label>
            </div>
          </div>

          {(label || description) && (
            <div className="flex-1 pt-0.5">
              {label && (
                <label
                  htmlFor={checkboxId}
                  className="block text-sm font-medium text-[hsl(var(--text-primary))] cursor-pointer"
                >
                  {label}
                </label>
              )}
              {description && (
                <p
                  id={descriptionId}
                  className="text-sm text-[hsl(var(--text-secondary))] mt-0.5"
                >
                  {description}
                </p>
              )}
            </div>
          )}
        </div>

        {error && (
          <p
            id={errorId}
            className="mt-1.5 ml-8 text-sm text-[hsl(var(--danger-fg))] flex items-start gap-1"
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
      </div>
    );
  }
);

Checkbox.displayName = 'Checkbox';

export default Checkbox;
