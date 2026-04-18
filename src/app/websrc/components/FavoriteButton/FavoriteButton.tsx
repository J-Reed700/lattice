/**
 * FavoriteButton Component
 *
 * A star icon button that allows users to favorite/unfavorite documents.
 * Features:
 * - Filled star when favorited, outline when not
 * - Smooth animation on toggle
 * - Optimistic UI updates
 * - Tooltip showing current state
 * - Works in DocumentViewer and SearchResults
 */

import React, { memo, useCallback, useState } from 'react';

import { Star } from 'lucide-react';

import { useFavorites } from '../../contexts/FavoritesContext';
import { useToast } from '../ui/Toast';

interface FavoriteButtonProps {
  documentId: string;
  size?: 'sm' | 'md' | 'lg';
  showLabel?: boolean;
  className?: string;
}

export const FavoriteButton = memo(({
  documentId,
  size = 'md',
  showLabel = false,
  className = '',
}: FavoriteButtonProps) => {
  const { isFavorite: checkIsFavorite, addFavorite, removeFavorite } = useFavorites();
  const isFavorite = checkIsFavorite(documentId);
  const { addToast } = useToast();
  const [isAnimating, setIsAnimating] = useState(false);

  const handleClick = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      e.preventDefault();

      // Trigger animation
      setIsAnimating(true);
      setTimeout(() => setIsAnimating(false), 300);

      try {
        if (isFavorite) {
          await removeFavorite(documentId);
          addToast('Removed from favorites', 'success', 2000);
        } else {
          await addFavorite(documentId);
          addToast('Added to favorites', 'success', 2000);
        }
      } catch {
        addToast(
          isFavorite
            ? 'Failed to remove favorite'
            : 'Failed to add favorite',
          'error',
          3000
        );
      }
    },
    [isFavorite, documentId, addFavorite, removeFavorite, addToast]
  );

  const sizeClasses = {
    sm: 'w-4 h-4',
    md: 'w-5 h-5',
    lg: 'w-6 h-6',
  };

  const buttonSizeClasses = {
    sm: 'p-1',
    md: 'p-1.5',
    lg: 'p-2',
  };

  const textSizeClasses = {
    sm: 'text-xs',
    md: 'text-sm',
    lg: 'text-base',
  };

  return (
    <button
      onClick={handleClick}
      className={`
        inline-flex items-center gap-1.5 rounded-md
        transition-colors duration-200 ease-out
        hover:bg-[hsl(var(--surface-raised))]
        focus:outline-none focus:ring-2 focus:ring-[hsl(var(--warning-fg))] focus:ring-offset-1
        ${buttonSizeClasses[size]}
        ${className}
      `}
      title={isFavorite ? 'Remove from favorites' : 'Add to favorites'}
      aria-label={isFavorite ? 'Remove from favorites' : 'Add to favorites'}
    >
      <Star
        className={`
          ${sizeClasses[size]}
          transition-colors duration-200 ease-out
          ${
            isFavorite
              ? 'fill-[hsl(var(--warning-fg))] text-[hsl(var(--warning-fg))]'
              : 'fill-none text-[hsl(var(--text-tertiary))]'
          }
          ${isAnimating ? 'scale-125' : 'scale-100'}
          hover:text-[hsl(var(--warning-fg))] hover:fill-[hsl(var(--warning-muted))]
        `}
        strokeWidth={2}
      />
      {showLabel && (
        <span
          className={`
            ${textSizeClasses[size]}
            text-[hsl(var(--text-secondary))]
            font-medium
          `}
        >
          {isFavorite ? 'Favorited' : 'Favorite'}
        </span>
      )}
    </button>
  );
});
