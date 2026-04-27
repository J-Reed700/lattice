/**
 * Skeleton
 *
 * Purpose: Placeholder loading states that match content structure
 *
 * Features:
 * - Multiple preset variants (text, rect, circle)
 * - Customizable dimensions
 * - Smooth pulse animation
 * - Dark mode support
 * - Respects prefers-reduced-motion
 *
 * Usage:
 * - Use to prevent layout shift during loading
 * - Match skeleton to actual content dimensions
 * - Group multiple skeletons to represent complex layouts
 *
 * Accessibility: WCAG AA, reduced motion support, proper ARIA labels
 */

export interface SkeletonProps {
  variant?: 'text' | 'rect' | 'circle';
  width?: string | number;
  height?: string | number;
  className?: string;
  animate?: boolean;
}

export function Skeleton({
  variant = 'text',
  width,
  height,
  className = '',
  animate = true,
}: SkeletonProps) {
  const variantStyles = {
    text: 'rounded',
    rect: 'rounded-md',
    circle: 'rounded-full',
  };

  const defaultSizes = {
    text: { width: '100%', height: '1rem' },
    rect: { width: '100%', height: '8rem' },
    circle: { width: '3rem', height: '3rem' },
  };

  const size = {
    width: width ?? defaultSizes[variant].width,
    height: height ?? defaultSizes[variant].height,
  };

  const style = {
    width: typeof size.width === 'number' ? `${size.width}px` : size.width,
    height: typeof size.height === 'number' ? `${size.height}px` : size.height,
  };

  return (
    <div
      className={`
        bg-[hsl(var(--surface-raised))]
        ${variantStyles[variant]}
        ${animate ? 'animate-pulse' : ''}
        ${className}
      `}
      style={style}
      role="status"
      aria-label="Loading..."
      aria-live="polite"
    >
      <span className="sr-only">Loading...</span>
    </div>
  );
}

/**
 * SkeletonText
 *
 * Purpose: Multi-line text skeleton for paragraphs
 */
interface SkeletonTextProps {
  lines?: number;
  lastLineWidth?: string;
  className?: string;
}

export function SkeletonText({ lines = 3, lastLineWidth = '75%', className = '' }: SkeletonTextProps) {
  return (
    <div className={`space-y-2 ${className}`}>
      {Array.from({ length: lines }, (_, i) => (
        <Skeleton
          key={i}
          variant="text"
          width={i === lines - 1 ? lastLineWidth : '100%'}
          height="0.875rem"
        />
      ))}
    </div>
  );
}

/**
 * SkeletonCard
 *
 * Purpose: Card-shaped skeleton for list items and cards
 */
interface SkeletonCardProps {
  showAvatar?: boolean;
  showActions?: boolean;
  className?: string;
}

export function SkeletonCard({ showAvatar = false, showActions = false, className = '' }: SkeletonCardProps) {
  return (
    <div
      className={`bg-[hsl(var(--surface))] border border-[hsl(var(--border-subtle))] rounded-md p-4 ${className}`}
    >
      <div className="flex items-start gap-3">
        {showAvatar && <Skeleton variant="circle" width={40} height={40} />}
        <div className="flex-1 space-y-3">
          <Skeleton variant="text" width="60%" height="1.25rem" />
          <SkeletonText lines={2} lastLineWidth="85%" />
          {showActions && (
            <div className="flex gap-2 pt-2">
              <Skeleton variant="rect" width={80} height={24} />
              <Skeleton variant="rect" width={80} height={24} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * SkeletonList
 *
 * Purpose: Multiple skeleton cards for list loading states
 */
interface SkeletonListProps {
  count?: number;
  showAvatar?: boolean;
  showActions?: boolean;
  className?: string;
}

export function SkeletonList({ count = 3, showAvatar = false, showActions = false, className = '' }: SkeletonListProps) {
  return (
    <div className={`space-y-4 ${className}`}>
      {Array.from({ length: count }, (_, i) => (
        <SkeletonCard key={i} showAvatar={showAvatar} showActions={showActions} />
      ))}
    </div>
  );
}

/**
 * SkeletonTable
 *
 * Purpose: Table skeleton for data grids
 */
interface SkeletonTableProps {
  rows?: number;
  columns?: number;
  className?: string;
}

export function SkeletonTable({ rows = 5, columns = 4, className = '' }: SkeletonTableProps) {
  return (
    <div className={`space-y-2 ${className}`}>
      {/* Header */}
      <div className="flex gap-4 pb-2 border-b border-[hsl(var(--border-subtle))]">
        {Array.from({ length: columns }, (_, i) => (
          <Skeleton key={`header-${i}`} variant="text" width={`${100 / columns}%`} height="1rem" />
        ))}
      </div>
      {/* Rows */}
      {Array.from({ length: rows }, (_, rowIdx) => (
        <div key={`row-${rowIdx}`} className="flex gap-4 py-2">
          {Array.from({ length: columns }, (_, colIdx) => (
            <Skeleton key={`cell-${rowIdx}-${colIdx}`} variant="text" width={`${100 / columns}%`} height="0.875rem" />
          ))}
        </div>
      ))}
    </div>
  );
}

