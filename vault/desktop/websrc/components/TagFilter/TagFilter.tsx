import { useState, useEffect, useCallback } from 'react';

import { type Tag, type TagWithCount } from '@/types';

import VaultAPI from '../../lib/api';


interface TagFilterProps {
  onTagSelect?: (tag: Tag | null) => void;
  selectedTag?: Tag | null;
  className?: string;
}

export function TagFilter({ onTagSelect, selectedTag, className = '' }: TagFilterProps) {
  const [tags, setTags] = useState<TagWithCount[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);

  const loadTags = useCallback(async () => {
    setIsLoading(true);
    const result = await VaultAPI.getAllTagsWithCounts();

    if (result.ok) {
      setTags(result.data);
      setError(null);
    } else {
      console.error('Failed to load tags:', result.error);
      setError('Failed to load tags');
    }

    setIsLoading(false);
  }, []); // No dependencies - stable function

  useEffect(() => {
    void loadTags();
  }, [loadTags]);

  const handleTagClick = useCallback((tag: TagWithCount) => {
    if (selectedTag?.id === tag.id) {
      // Deselect if clicking the same tag
      onTagSelect?.(null);
    } else {
      // Select the clicked tag - Tag requires documentCount
      onTagSelect?.({
        id: tag.id,
        name: tag.name,
        color: tag.color ?? '#3b82f6',
        documentCount: tag.document_count,
        createdAt: tag.created_at,
        updatedAt: tag.created_at
      });
    }
  }, [selectedTag, onTagSelect]);

  const handleClearFilter = useCallback(() => {
    onTagSelect?.(null);
  }, [onTagSelect]);

  const displayedTags = showAll ? tags : tags.slice(0, 10);
  const hasMoreTags = tags.length > 10;

  if (isLoading) {
    return (
      <div className={`tag-filter ${className}`}>
        <div className="text-sm text-[var(--text-secondary)]">Loading tags...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className={`tag-filter ${className}`}>
        <div className="text-sm text-[var(--error)]">{error}</div>
      </div>
    );
  }

  if (tags.length === 0) {
    return null; // Don't show filter if no tags exist
  }

  return (
    <div className={`tag-filter ${className}`}>
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-sm font-semibold text-[var(--text-secondary)]">
          Filter by Tag
        </h3>
        {selectedTag && (
          <button
            onClick={handleClearFilter}
            className="text-xs text-[var(--accent-primary)] hover:underline"
          >
            Clear filter
          </button>
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        {displayedTags.map(tag => (
          <button
            key={tag.id}
            onClick={() => handleTagClick(tag)}
            className={`
              inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-sm
              transition-all duration-200
              ${selectedTag?.id === tag.id
                ? 'ring-2 ring-[var(--accent-primary)] ring-offset-2'
                : 'hover:opacity-80'
              }
            `}
            style={{
              backgroundColor: selectedTag?.id === tag.id ? (tag.color ?? '#3b82f6') : `${tag.color ?? '#3b82f6'}20`,
              color: selectedTag?.id === tag.id ? '#ffffff' : (tag.color ?? '#3b82f6'),
              borderColor: tag.color ?? '#3b82f6',
              borderWidth: '1px'
            }}
          >
            <span>{tag.name}</span>
            <span className="text-xs opacity-75">({tag.document_count})</span>
          </button>
        ))}
      </div>

      {hasMoreTags && (
        <button
          onClick={() => setShowAll(!showAll)}
          className="mt-2 text-xs text-[var(--accent-primary)] hover:underline"
        >
          {showAll ? 'Show less' : `Show ${tags.length - 10} more...`}
        </button>
      )}
    </div>
  );
}
