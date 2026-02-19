import { forwardRef, type InputHTMLAttributes } from 'react';

/**
 * Switch
 *
 * Purpose: Toggle control for binary on/off settings
 *
 * Features:
 * - Smooth animation transitions
 * - Keyboard accessible (Space/Enter to toggle)
 * - Clear visual states (on/off)
 * - Label support with description
 * - Dark mode support
 *
 * States: default, hover, focus, checked, disabled
 * Accessibility: WCAG AA, keyboard navigation, ARIA attributes
 */

interface SwitchProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label?: string;
  description?: string;
  checked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
}

const Switch = forwardRef<HTMLInputElement, SwitchProps>(
  (
    {
      label,
      description,
      checked = false,
      onCheckedChange,
      disabled,
      className = '',
      id,
      ...props
    },
    ref
  ) => {
    const switchId = id || `switch-${Math.random().toString(36).substr(2, 9)}`;

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      onCheckedChange?.(e.target.checked);
      props.onChange?.(e);
    };

    return (
      <div className={`flex items-start gap-3 ${className}`}>
        <button
          type="button"
          role="switch"
          aria-checked={checked}
          aria-labelledby={label ? `${switchId}-label` : undefined}
          aria-describedby={description ? `${switchId}-description` : undefined}
          disabled={disabled}
          onClick={() => !disabled && onCheckedChange?.(!checked)}
          className={`
            relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full
            transition-colors duration-200 ease-in-out
            focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)] focus-visible:ring-offset-2
            disabled:opacity-50 disabled:cursor-not-allowed
            ${
              checked
                ? 'bg-[var(--accent-primary)]'
                : 'bg-[var(--bg-tertiary)]'
            }
          `}
        >
          <input
            ref={ref}
            id={switchId}
            type="checkbox"
            checked={checked}
            onChange={handleChange}
            disabled={disabled}
            className="sr-only"
            {...props}
          />
          <span
            aria-hidden="true"
            className={`
              pointer-events-none inline-block h-5 w-5 transform rounded-full
              bg-[var(--surface-elevated)] shadow ring-0 transition duration-200 ease-in-out
              ${checked ? 'translate-x-5' : 'translate-x-0.5'}
              mt-0.5
            `}
          />
        </button>

        {(label || description) && (
          <div className="flex-1">
            {label && (
              <label
                id={`${switchId}-label`}
                htmlFor={switchId}
                className="block text-sm font-medium text-[var(--text-primary)] cursor-pointer"
              >
                {label}
              </label>
            )}
            {description && (
              <p
                id={`${switchId}-description`}
                className="text-sm text-[var(--text-secondary)] mt-0.5"
              >
                {description}
              </p>
            )}
          </div>
        )}
      </div>
    );
  }
);

Switch.displayName = 'Switch';

export default Switch;
