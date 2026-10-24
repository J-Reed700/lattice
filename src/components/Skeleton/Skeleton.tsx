import React from 'react';

export interface SkeletonProps {
  width?: string | number;
  height?: string | number;
  circle?: boolean;
  className?: string;
  count?: number;
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
    ...(circle && widthStyle && { width: heightStyle }),
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

export interface SkeletonTextProps {
  lines?: number;
  lastLineWidth?: string;
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

export interface SkeletonAvatarProps {
  size?: 'sm' | 'md' | 'lg';
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

export interface SkeletonButtonProps {
  size?: 'sm' | 'md' | 'lg';
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

export const SkeletonCard: React.FC<{ className?: string }> = ({ className = '' }) => (
    <div
      className={`bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] p-6 ${className}`}
      aria-hidden="true"
    >
      <div className="space-y-4">
        <Skeleton height="1.5rem" width="75%" />

        <SkeletonText lines={3} />

        <div className="flex gap-3 pt-2">
          <SkeletonButton size="sm" />
          <SkeletonButton size="sm" />
        </div>
      </div>
    </div>
  );
