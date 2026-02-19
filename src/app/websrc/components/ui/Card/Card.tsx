import { type HTMLAttributes, type MouseEventHandler, forwardRef } from 'react';

/**
 * Card
 *
 * Purpose: Container component for grouping related content
 *
 * Features:
 * - Clickable variant with hover effects and micro-interactions
 * - Glassmorphism variant for floating UI
 * - Dark mode support
 * - Flexible padding options
 * - Elevation system for depth hierarchy
 *
 * Use Cases:
 * - Search results
 * - Document listings
 * - Settings panels
 * - Dashboard widgets
 *
 * States: default, hover (if clickable), focus (if clickable)
 * Accessibility: Semantic HTML, keyboard navigation (if clickable)
 * Micro-interactions: Scale and lift with CSS transitions
 */

interface CardProps extends Omit<HTMLAttributes<HTMLDivElement>, 'onClick'> {
  variant?: 'default' | 'clickable' | 'glass' | 'bento';
  padding?: 'none' | 'sm' | 'md' | 'lg';
  asChild?: boolean;
  onClick?: MouseEventHandler<HTMLDivElement | HTMLButtonElement>;
}

const Card = forwardRef<HTMLDivElement, CardProps>(
  (
    {
      children,
      variant = 'default',
      padding = 'md',
      className = '',
      onClick,
      ...props
    },
    ref
  ) => {
    const baseStyles = 'rounded-lg transition-all duration-200 ease-out motion-reduce:transition-none';

    const variantStyles = {
      default: 'bg-[var(--surface-elevated)] border border-[var(--border-color)] elevation-1',
      clickable: 'bg-[var(--surface-elevated)] border border-[var(--border-color)] elevation-1 hover:elevation-2 cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)] focus-visible:ring-offset-2 hover:scale-[1.01] hover:-translate-y-0.5 active:scale-[0.99] active:translate-y-0',
      glass: 'glass elevation-2 border-accent-top',
      bento: 'bento-card',
    };

    const paddingStyles = {
      none: '',
      sm: 'p-3',
      md: 'p-5',
      lg: 'p-6',
    };

    const isClickable = variant === 'clickable' || onClick;
    const computedVariant = isClickable && variant === 'default' ? 'clickable' : variant;

    if (isClickable) {
      // Only spread button-compatible props
      const { ...divProps } = props;
      return (
        <button
          ref={ref as React.Ref<HTMLButtonElement>}
          onClick={onClick as MouseEventHandler<HTMLButtonElement>}
          type="button"
          className={`${baseStyles} ${variantStyles[computedVariant]} ${paddingStyles[padding]} ${className}`}
          {...(divProps as React.HTMLAttributes<HTMLButtonElement>)}
        >
          {children}
        </button>
      );
    }

    return (
      <div
        ref={ref}
        className={`${baseStyles} ${variantStyles[computedVariant]} ${paddingStyles[padding]} ${className}`}
        {...props}
      >
        {children}
      </div>
    );
  }
);

Card.displayName = 'Card';

interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {}

export const CardHeader = forwardRef<HTMLDivElement, CardHeaderProps>(
  ({ className = '', children, ...props }, ref) => (
      <div
        ref={ref}
        className={`flex flex-col space-y-1.5 ${className}`}
        {...props}
      >
        {children}
      </div>
    )
);

CardHeader.displayName = 'CardHeader';

interface CardTitleProps extends HTMLAttributes<HTMLHeadingElement> {}

export const CardTitle = forwardRef<HTMLHeadingElement, CardTitleProps>(
  ({ className = '', children, ...props }, ref) => (
      <h3
        ref={ref}
        className={`text-lg font-semibold leading-none tracking-tight ${className}`}
        {...props}
      >
        {children}
      </h3>
    )
);

CardTitle.displayName = 'CardTitle';

interface CardDescriptionProps extends HTMLAttributes<HTMLParagraphElement> {}

export const CardDescription = forwardRef<HTMLParagraphElement, CardDescriptionProps>(
  ({ className = '', children, ...props }, ref) => (
      <p
        ref={ref}
        className={`text-sm text-[var(--text-secondary)] ${className}`}
        {...props}
      >
        {children}
      </p>
    )
);

CardDescription.displayName = 'CardDescription';

interface CardContentProps extends HTMLAttributes<HTMLDivElement> {}

export const CardContent = forwardRef<HTMLDivElement, CardContentProps>(
  ({ className = '', children, ...props }, ref) => (
      <div
        ref={ref}
        className={className}
        {...props}
      >
        {children}
      </div>
    )
);

CardContent.displayName = 'CardContent';

export default Card;
