/**
 * Skeleton Component
 *
 * Purpose: Generic skeleton loader for placeholder content while loading
 *
 * Features:
 * - Animated shimmer effect
 * - Multiple shapes (rectangle, circle)
 * - Flexible sizing
 * - Multiple instances with count prop
 * - Dark mode support
 *
 * States: loading only
 * Accessibility: Hidden from screen readers (decorative)
 * Performance: GPU-accelerated CSS animations at 60fps
 */

import React from 'react';

export interface SkeletonProps {
  /** Width of skeleton (CSS value or number for pixels) */
  width?: string | number;

  /** Height of skeleton (CSS value or number for pixels) */
  height?: string | number;

  /** Whether skeleton should be circular */
  circle?: boolean;

  /** Additional CSS classes */
  className?: string;

  /** Number of skeleton instances to render */
  count?: number;

  /** Vertical spacing between multiple instances (Tailwind spacing class) */
  gap?: string;
}

export const Skeleton: React.FC<SkeletonProps> = ({
  width,
  height = '1rem',
  circle = false,
  className = '',
  count = 1,
  gap = 'space-y-3',
}) => {
  const widthStyle = typeof width === 'number' ? `${width}px` : width;
  const heightStyle = typeof height === 'number' ? `${height}px` : height;

  const skeletonClass = [
    'skeleton',
    circle ? 'rounded-full' : 'rounded',
    'bg-[hsl(var(--surface-raised))]',
    'animate-pulse',
    className,
  ].filter(Boolean).join(' ');

  const style: React.CSSProperties = {
    width: widthStyle,
    height: heightStyle,
    ...(circle && widthStyle && { width: heightStyle }), // Ensure circle is square
  };

  if (count === 1) {
    return (
      <div
        className={skeletonClass}
        style={style}
        aria-hidden="true"
        role="presentation"
      />
    );
  }

  return (
    <div className={gap} aria-hidden="true" role="presentation">
      {Array.from({ length: count }).map((_, index) => (
        <div
          key={index}
          className={skeletonClass}
          style={style}
        />
      ))}
    </div>
  );
};

/**
 * SkeletonText - Preset for text lines
 */
export interface SkeletonTextProps {
  /** Number of text lines */
  lines?: number;

  /** Width of last line (to create natural text appearance) */
  lastLineWidth?: string;

  /** Additional CSS classes */
  className?: string;
}

export const SkeletonText: React.FC<SkeletonTextProps> = ({
  lines = 3,
  lastLineWidth = '80%',
  className = '',
}) => (
    <div className={`space-y-2 ${className}`} aria-hidden="true">
      {Array.from({ length: lines }).map((_, index) => (
        <Skeleton
          key={index}
          height="0.875rem"
          width={index === lines - 1 ? lastLineWidth : '100%'}
        />
      ))}
    </div>
  );

/**
 * SkeletonAvatar - Preset for circular avatar
 */
export interface SkeletonAvatarProps {
  /** Size of avatar */
  size?: 'sm' | 'md' | 'lg';

  /** Additional CSS classes */
  className?: string;
}

export const SkeletonAvatar: React.FC<SkeletonAvatarProps> = ({
  size = 'md',
  className = '',
}) => {
  const sizeMap = {
    sm: 32,
    md: 40,
    lg: 64,
  };

  return (
    <Skeleton
      circle
      width={sizeMap[size]}
      height={sizeMap[size]}
      className={className}
    />
  );
};

/**
 * SkeletonButton - Preset for button skeleton
 */
export interface SkeletonButtonProps {
  /** Button size */
  size?: 'sm' | 'md' | 'lg';

  /** Additional CSS classes */
  className?: string;
}

export const SkeletonButton: React.FC<SkeletonButtonProps> = ({
  size = 'md',
  className = '',
}) => {
  const sizeMap = {
    sm: { width: '80px', height: '32px' },
    md: { width: '100px', height: '40px' },
    lg: { width: '120px', height: '48px' },
  };

  return (
    <Skeleton
      width={sizeMap[size].width}
      height={sizeMap[size].height}
      className={className}
    />
  );
};

/**
 * SkeletonCard - Preset for card layout skeleton
 */
export const SkeletonCard: React.FC<{ className?: string }> = ({ className = '' }) => (
    <div
      className={`bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] p-6 ${className}`}
      aria-hidden="true"
    >
      <div className="space-y-4">
        {/* Title */}
        <Skeleton height="1.5rem" width="75%" />

        {/* Content lines */}
        <SkeletonText lines={3} />

        {/* Footer buttons */}
        <div className="flex gap-3 pt-2">
          <SkeletonButton size="sm" />
          <SkeletonButton size="sm" />
        </div>
      </div>
    </div>
  );
