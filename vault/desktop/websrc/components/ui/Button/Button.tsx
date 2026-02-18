import { type ComponentPropsWithoutRef, forwardRef } from 'react';

import { motion } from 'framer-motion';

/**
 * Button
 *
 * Purpose: Primary interaction element for triggering actions
 *
 * Variants:
 * - primary: Main CTAs (gradient background with glow)
 * - secondary: Alternative actions (gray background)
 * - ghost: Subtle actions (transparent background)
 * - danger: Destructive actions (red background)
 *
 * States: default, hover, active, disabled, loading
 * Accessibility: WCAG AA, keyboard navigation, focus visible
 * Micro-interactions: Scale, glow effects with Framer Motion
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
    const baseStyles = 'inline-flex items-center justify-center font-medium rounded-lg transition-all duration-200 ease-out focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed select-none motion-reduce:transition-none';

    const variantStyles = {
      primary: 'gradient-brand text-white hover:shadow-glow transition-shadow focus-visible:ring-[var(--accent-primary)] elevation-1 hover:elevation-2',
      secondary: 'bg-[var(--bg-secondary)] text-[var(--text-primary)] hover:bg-[var(--surface-hover)] elevation-1 hover:elevation-2 focus-visible:ring-[var(--accent-primary)]',
      ghost: 'bg-transparent text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] active:bg-[var(--surface-active)] focus-visible:ring-[var(--accent-primary)]',
      danger: 'bg-[var(--error)] text-white hover:opacity-90 elevation-1 hover:elevation-2 focus-visible:ring-[var(--error)]',
    };

    const sizeStyles = {
      sm: 'px-3 py-2.5 text-sm gap-1.5 min-h-[44px]',
      md: 'px-4 py-2.5 text-base gap-2 min-h-[44px]',
      lg: 'px-6 py-3 text-lg gap-2.5 min-h-[48px]',
    };

    const isDisabled = disabled || isLoading;

    return (
      <motion.button
        ref={ref}
        disabled={isDisabled}
        className={`${baseStyles} ${variantStyles[variant]} ${sizeStyles[size]} ${className}`}
        whileHover={!isDisabled ? { scale: 1.02, y: -2 } : undefined}
        whileTap={!isDisabled ? { scale: 0.98, y: 0 } : undefined}
        {...props}
      >
        {isLoading && (
          <motion.div
            initial={{ opacity: 0, scale: 0 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0 }}
            transition={{ duration: 0.2 }}
            className="mr-2"
          >
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
          </motion.div>
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
