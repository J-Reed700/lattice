import { type HTMLAttributes, type MouseEventHandler, forwardRef } from 'react';

/**
 * Card
 *
 * Purpose: Container component for grouping related content
 *
 * Features:
 * - Clickable variant with hover effects
 * - Dark mode support
 * - Flexible padding options
 *
 * Use Cases:
 * - Search results
 * - Document listings
 * - Settings panels
 * - Dashboard widgets
 *
 * States: default, hover (if clickable), focus (if clickable)
 * Accessibility: Semantic HTML, keyboard navigation (if clickable)
 */

interface CardProps extends Omit<HTMLAttributes<HTMLDivElement>, 'onClick'> {
  variant?: 'default' | 'clickable';
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
    const baseStyles = 'rounded-lg transition-colors duration-fast ease-out motion-reduce:transition-none';

    const variantStyles = {
      default: 'bg-[hsl(var(--surface))] border border-[hsl(var(--border-subtle))]',
      clickable: 'bg-[hsl(var(--surface))] border border-[hsl(var(--border-subtle))] hover:bg-[hsl(var(--surface-raised))] cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]',
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
        className={`text-lg font-semibold leading-none tracking-tight text-[hsl(var(--text-primary))] ${className}`}
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
        className={`text-sm text-[hsl(var(--text-secondary))] ${className}`}
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
