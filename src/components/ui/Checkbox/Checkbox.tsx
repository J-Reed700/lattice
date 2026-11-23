import { forwardRef, useEffect, useId, useRef, useState, type InputHTMLAttributes } from 'react';

import { Check, Minus } from 'lucide-react';

import { cn } from '@/lib/utils';

/**
 * Checkbox
 *
 * Accessible minimalist checkbox supporting controlled, uncontrolled,
 * indeterminate, description, and error states. Uses React `useId()` for
 * stable deterministic IDs.
 */

export interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
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
      defaultChecked,
      onCheckedChange,
      onChange,
      disabled,
      className,
      id,
      ...props
    },
    ref
  ) => {
    const generatedId = useId();
    const checkboxId = id ?? `checkbox-${generatedId}`;
    const errorId = error ? `${checkboxId}-error` : undefined;
    const descriptionId = description ? `${checkboxId}-description` : undefined;
    const describedBy = [descriptionId, errorId].filter(Boolean).join(' ') || undefined;
    const hasError = Boolean(error);

    const isControlled = checked !== undefined;
    const [uncontrolledChecked, setUncontrolledChecked] = useState(Boolean(defaultChecked));
    const isChecked = isControlled ? Boolean(checked) : uncontrolledChecked;

    const internalRef = useRef<HTMLInputElement | null>(null);

    useEffect(() => {
      if (internalRef.current) {
        internalRef.current.indeterminate = indeterminate;
      }
    }, [indeterminate]);

    const setRefs = (node: HTMLInputElement | null) => {
      internalRef.current = node;
      if (typeof ref === 'function') {
        ref(node);
      } else if (ref) {
        ref.current = node;
      }
    };

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      if (!isControlled) {
        setUncontrolledChecked(e.target.checked);
      }
      onCheckedChange?.(e.target.checked);
      onChange?.(e);
    };

    const isFilled = isChecked || indeterminate;

    return (
      <div className={className}>
        <div className="flex items-start gap-2.5">
          <div className="flex h-5 items-center">
            <div className="relative">
              <input
                ref={setRefs}
                id={checkboxId}
                type="checkbox"
                checked={isControlled ? checked : undefined}
                defaultChecked={isControlled ? undefined : defaultChecked}
                onChange={handleChange}
                disabled={disabled}
                aria-checked={indeterminate ? 'mixed' : undefined}
                aria-invalid={hasError}
                aria-describedby={describedBy}
                className="peer sr-only"
                {...props}
              />
              <label
                htmlFor={checkboxId}
                className={cn(
                  'flex h-4 w-4 items-center justify-center rounded-sm border transition-[background-color,border-color,box-shadow] duration-fast',
                  'peer-focus-visible:ring-2 peer-focus-visible:ring-ring peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-bg',
                  isFilled
                    ? 'border-accent bg-accent text-accent-fg shadow-none'
                    : hasError
                      ? 'border-danger-fg bg-surface shadow-control'
                      : 'border-border-strong bg-surface shadow-control',
                  disabled
                    ? 'cursor-not-allowed opacity-50'
                    : isFilled
                      ? 'cursor-pointer hover:bg-accent-hover hover:border-accent-hover'
                      : 'cursor-pointer hover:border-accent',
                )}
              >
                {isChecked && !indeterminate && (
                  <Check className="h-3 w-3 text-accent-fg" strokeWidth={2.75} aria-hidden="true" />
                )}
                {indeterminate && (
                  <Minus className="h-3 w-3 text-accent-fg" strokeWidth={2.75} aria-hidden="true" />
                )}
              </label>
            </div>
          </div>

          {(label || description) && (
            <div className="min-w-0 flex-1">
              {label && (
                <label
                  htmlFor={checkboxId}
                  className={cn(
                    'block text-ui font-medium leading-5 text-text-primary',
                    disabled ? 'cursor-not-allowed opacity-50' : 'cursor-pointer',
                  )}
                >
                  {label}
                </label>
              )}
              {description && (
                <p
                  id={descriptionId}
                  className="mt-0.5 text-xs leading-relaxed text-text-secondary"
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
            className="mt-1.5 ml-6.5 flex items-start gap-1 text-xs text-danger-fg"
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
      </div>
    );
  }
);

Checkbox.displayName = 'Checkbox';

export { Checkbox };
export default Checkbox;
