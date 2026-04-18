import { type ComponentPropsWithoutRef, forwardRef } from 'react';

import { motion } from 'framer-motion';

/**
 * Button
 *
 * Purpose: Primary interaction element for triggering actions
 *
 * Variants:
 * - primary: Main CTAs (accent fill)
 * - secondary: Alternative actions (surface fill)
 * - ghost: Subtle actions (transparent)
 * - danger: Destructive actions (danger fill)
 *
 * States: default, hover, active, disabled, loading
 * Accessibility: WCAG AA, keyboard navigation, focus visible
 */

interface ButtonProps extends Omit<ComponentPropsWithoutRef<'button'>, 'onDrag' | 'onDragStart' | 'onDragEnd' | 'onAnimationStart' | 'onAnimationEnd'> {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  size?: 'sm' | 'md' | 'lg';
  isLoading?: boolean;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      children,
      variant = 'primary',
      size = 'md',
      isLoading = false,
      leftIcon,
      rightIcon,
      disabled,
      className = '',
      ...props
    },
    ref
  ) => {
    const baseStyles = 'inline-flex items-center justify-center font-medium rounded-md transition-colors duration-fast ease-out focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed select-none motion-reduce:transition-none';

    const variantStyles = {
      primary: 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))]',
      secondary: 'bg-surface text-[hsl(var(--text-primary))] border border-default hover:bg-surface-raised',
      ghost: 'bg-transparent text-[hsl(var(--text-secondary))] hover:bg-surface hover:text-[hsl(var(--text-primary))]',
      danger: 'bg-[hsl(var(--danger))] text-[hsl(var(--accent-fg))] hover:opacity-90',
    };

    const sizeStyles = {
      sm: 'px-3 py-1.5 text-sm gap-1.5 min-h-[32px]',
      md: 'px-4 py-2 text-sm gap-2 min-h-[36px]',
      lg: 'px-6 py-2.5 text-base gap-2.5 min-h-[40px]',
    };

    const isDisabled = disabled || isLoading;

    return (
      <motion.button
        ref={ref}
        disabled={isDisabled}
        className={`${baseStyles} ${variantStyles[variant]} ${sizeStyles[size]} ${className}`}
        whileTap={!isDisabled ? { scale: 0.97 } : undefined}
        {...props}
      >
        {isLoading && (
          <span className="mr-2 inline-flex">
            <svg
              className="animate-spin h-4 w-4"
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
          </span>
        )}
        {!isLoading && leftIcon && <span className="flex-shrink-0">{leftIcon}</span>}
        <span>{children}</span>
        {!isLoading && rightIcon && <span className="flex-shrink-0">{rightIcon}</span>}
      </motion.button>
    );
  }
);

Button.displayName = 'Button';

export default Button;
