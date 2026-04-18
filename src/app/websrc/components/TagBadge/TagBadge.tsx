import React, { memo, useCallback } from 'react';

import type { Tag } from '../../types/api/tags';

interface TagBadgeProps {
  tag: Tag;
  onRemove?: (tagId: string) => void;
  onClick?: (tag: Tag) => void;
  removable?: boolean;
  size?: 'sm' | 'md' | 'lg';
}

export const TagBadge = memo(({
  tag,
  onRemove,
  onClick,
  removable = false,
  size = 'md'
}: TagBadgeProps) => {
  const handleClick = useCallback((e: React.MouseEvent) => {
    if (onClick && !removable) {
      e.stopPropagation();
      onClick(tag);
    }
  }, [onClick, removable, tag]);

  const handleRemove = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onRemove?.(tag.id);
  }, [onRemove, tag.id]);

  const sizeClasses = {
    sm: 'text-xs px-2 py-0.5',
    md: 'text-sm px-2.5 py-1',
    lg: 'text-base px-3 py-1.5'
  };

  const iconSizeClasses = {
    sm: 'w-3 h-3',
    md: 'w-3.5 h-3.5',
    lg: 'w-4 h-4'
  };

  return (
    <span
      className={`
        inline-flex items-center gap-1.5 rounded-full font-medium
        transition-opacity duration-fast
        ${sizeClasses[size]}
        ${onClick && !removable ? 'cursor-pointer hover:opacity-80' : ''}
      `}
      style={{
        backgroundColor: `${tag.color}20`,
        color: tag.color,
        borderColor: tag.color,
        borderWidth: '1px'
      }}
      onClick={handleClick}
    >
      <span className="select-none">{tag.name}</span>
      {removable && onRemove && (
        <button
          onClick={handleRemove}
          className={`
            rounded-full hover:bg-[hsl(var(--surface-raised))]
            transition-colors duration-fast p-0.5
          `}
          aria-label={`Remove ${tag.name} tag`}
        >
          <svg
            className={iconSizeClasses[size]}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.75}
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      )}
    </span>
  );
});
