import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';

import VaultAPI from '../../lib/api';
import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { TagBadge, type Tag } from '../TagBadge';

interface TagManagerProps {
  documentId: string;
  className?: string;
}

export function TagManager({ documentId, className = '' }: TagManagerProps) {
  const [tags, setTags] = useState<Tag[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isGenerating, setIsGenerating] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const [showAutocomplete, setShowAutocomplete] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isOperationInProgress, setIsOperationInProgress] = useState(false);

  // Use ref to track if all tags have been loaded to prevent infinite loops
  const allTagsLoadedRef = useRef(false);

  // Load document tags
  // Migrated to DDD command: get_tags_for_document
  const loadTags = useCallback(async () => {
    setIsLoading(true);
    const result = await VaultAPI.getDocumentTags({ documentId });

    if (result.ok) {
      setTags(result.data?.tags || []);
      setError(null);
    } else {
      console.error('Failed to load tags:', result.error);
      setError(result.error);
      setTags([]);
    }

    setIsLoading(false);
  }, [documentId]);

  // Load all tags for autocomplete - stable function with no dependencies
  const loadAllTags = useCallback(async () => {
    // Only load if we haven't loaded yet
    if (allTagsLoadedRef.current) {
      return;
    }

    const result = await VaultAPI.listAllTags();

    if (result.ok) {
      setAllTags(result.data?.tags || []);
      allTagsLoadedRef.current = true;
    } else {
      console.error('Failed to load all tags:', result.error);
      setAllTags([]);
    }
  }, []); // No dependencies - stable function

  // Force reload all tags (used after adding new tags)
  const forceReloadAllTags = useCallback(async () => {
    allTagsLoadedRef.current = false;
    await loadAllTags();
  }, [loadAllTags]);

  useEffect(() => {
    void loadTags();
    void loadAllTags();
  }, [loadTags, loadAllTags]);

  const handleGenerateTags = useCallback(async () => {
    if (isOperationInProgress || isGenerating) {
      setError('Please wait for the current operation to complete');
      return;
    }

    setIsGenerating(true);
    setIsOperationInProgress(true);
    setError(null);

    // Step 1: Generate tag suggestions using LLM
    const generateResult = await VaultAPI.generateTagsForDocument(documentId);

    if (!generateResult.ok) {
      console.error('Failed to generate tags:', generateResult.error);
      setError(generateResult.error);
      setIsGenerating(false);
      setIsOperationInProgress(false);
      return;
    }

    const suggestedTags = generateResult.data || [];

    // Step 2: Apply generated tags (if any)
    if (suggestedTags && suggestedTags.length > 0) {
      const applyResult = await VaultAPI.applyTags(documentId, suggestedTags);

      if (applyResult.ok) {
        setTags(applyResult.data || []);
        // Force reload all tags to update autocomplete with new tags
        await forceReloadAllTags();
      } else {
        console.error('Failed to apply tags:', applyResult.error);
        setError(applyResult.error);
      }
    }

    setIsGenerating(false);
    setIsOperationInProgress(false);
  }, [documentId, isOperationInProgress, isGenerating, forceReloadAllTags]);

  // Add tag manually
  // FIX: Prevent race conditions with auto-generate operations
  const handleAddTag = useCallback(async (tagName: string) => {
    if (!tagName.trim()) return;

    if (isOperationInProgress) {
      setError('Please wait for the current operation to complete');
      return;
    }

    setIsOperationInProgress(true);
    const result = await VaultAPI.applyTags(documentId, [tagName.trim().toLowerCase()]);

    if (result.ok) {
      setTags(result.data || []);
      setInputValue('');
      setShowAutocomplete(false);
      setError(null);
      // Force reload all tags to update autocomplete with new tags
      await forceReloadAllTags();
    } else {
      console.error('Failed to add tag:', result.error);
      setError(result.error);
    }

    setIsOperationInProgress(false);
  }, [documentId, isOperationInProgress, forceReloadAllTags]);

  // Remove tag
  const handleRemoveTag = useCallback(async (tagId: string) => {
    const result = await VaultAPI.removeTagFromDocument({ documentId, tagId });

    if (result.ok) {
      setTags(prevTags => (prevTags || []).filter(t => t.id !== tagId));
      setError(null);
    } else {
      console.error('Failed to remove tag:', result.error);
      setError(result.error);
    }
  }, [documentId]);

  // Handle input change
  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setInputValue(value);
    setShowAutocomplete(value.length > 0);
  }, []);

  // Handle key press
  const handleKeyPress = useMemo(
    () => handleAsyncEvent(async (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        await handleAddTag(inputValue);
      } else if (e.key === 'Escape') {
        setShowAutocomplete(false);
        setInputValue('');
      }
    }),
    [handleAddTag, inputValue]
  );

  // Filter autocomplete suggestions
  const filteredSuggestions = useMemo(() => (allTags || []).filter(tag =>
      !(tags || []).some(t => t.id === tag.id) && // Not already applied
      tag.name.toLowerCase().includes(inputValue.toLowerCase())
    ).slice(0, 5), [allTags, tags, inputValue]);

  // Memoized event handlers for JSX
  const handleGenerateTagsClick = useMemo(
    () => handleAsyncEvent(handleGenerateTags),
    [handleGenerateTags]
  );

  const handleAddButtonClick = useMemo(
    () => handleAsyncEvent(async () => {
      await handleAddTag(inputValue);
    }),
    [handleAddTag, inputValue]
  );

  const handleInputFocus = useCallback(() => {
    setShowAutocomplete(inputValue.length > 0);
  }, [inputValue]);

  const handleRemoveTagWrapper = useMemo(
    () => handleAsyncEvent(handleRemoveTag),
    [handleRemoveTag]
  );

  // Create stable handler for autocomplete tag selection
  const handleAutocompleteTagClick = useCallback((tagName: string) => handleAsyncEvent(async () => {
      await handleAddTag(tagName);
    })(), [handleAddTag]);

  return (
    <div className={`tag-manager ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-[hsl(var(--text-secondary))]">
          Tags
        </h3>
        <button
          onClick={handleGenerateTagsClick}
          disabled={isGenerating || isOperationInProgress}
          className="
            px-3 py-1 text-xs font-medium rounded-md
            bg-[hsl(var(--accent))] text-white
            hover:bg-[hsl(var(--accent-hover))] disabled:opacity-50 disabled:cursor-not-allowed
            transition-colors
          "
        >
          {isGenerating ? (
            <>
              <svg className="inline w-3 h-3 mr-1 animate-spin" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              Generating...
            </>
          ) : (
            <>
              <svg className="inline w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
                <path d="M13 6a3 3 0 11-6 0 3 3 0 016 0zM18 8a2 2 0 11-4 0 2 2 0 014 0zM14 15a4 4 0 00-8 0v3h8v-3zM6 8a2 2 0 11-4 0 2 2 0 014 0zM16 18v-3a5.972 5.972 0 00-.75-2.906A3.005 3.005 0 0119 15v3h-3zM4.75 12.094A5.973 5.973 0 004 15v3H1v-3a3 3 0 013.75-2.906z" />
              </svg>
              Auto-generate
            </>
          )}
        </button>
      </div>

      {/* Error message */}
      {error && (
        <div className="mb-3 p-2 bg-[hsl(var(--danger-muted))]/20 text-[hsl(var(--danger-fg))] text-xs rounded">
          {error}
        </div>
      )}

      {/* Tags display */}
      <div className="flex flex-wrap gap-2 mb-3 min-h-[32px]">
        {isLoading ? (
          <div className="text-xs text-[hsl(var(--text-secondary))]">Loading tags...</div>
        ) : tags && tags.length > 0 ? (
          tags.map(tag => (
            <TagBadge
              key={tag.id}
              tag={tag}
              onRemove={handleRemoveTagWrapper}
              removable
              size="sm"
            />
          ))
        ) : (
          <div className="text-xs text-[hsl(var(--text-secondary))] italic">
            No tags yet. Add tags manually or auto-generate them.
          </div>
        )}
      </div>

      {/* Input with autocomplete */}
      <div className="relative">
        <div className="flex gap-2">
          <input
            type="text"
            value={inputValue}
            onChange={handleInputChange}
            onKeyDown={handleKeyPress}
            onFocus={handleInputFocus}
            placeholder="Add tag..."
            className="
              flex-1 px-3 py-1.5 text-sm rounded-md
              bg-[hsl(var(--surface-raised))]
              border border-[hsl(var(--border-subtle))]
              text-[hsl(var(--text-primary))]
              placeholder-[hsl(var(--text-tertiary))]
              focus:outline-none focus:ring-2 focus:ring-[hsl(var(--accent))]
            "
          />
          <button
            onClick={handleAddButtonClick}
            disabled={!inputValue.trim()}
            className="
              px-3 py-1.5 text-sm font-medium rounded-md
              bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))]
              hover:bg-[hsl(var(--surface-raised))]
              disabled:opacity-50 disabled:cursor-not-allowed
              transition-colors
            "
          >
            Add
          </button>
        </div>

        {/* Autocomplete dropdown */}
        {showAutocomplete && filteredSuggestions.length > 0 && (
          <div className="
            absolute z-10 mt-1 w-full
            bg-[hsl(var(--surface-raised))]
            border border-[hsl(var(--border-subtle))]
            rounded-md shadow-lg
            max-h-40 overflow-y-auto
          ">
            {filteredSuggestions.map(tag => (
              <button
                key={tag.id}
                onClick={() => handleAutocompleteTagClick(tag.name)}
                className="
                  w-full text-left px-3 py-2 text-sm
                  hover:bg-[hsl(var(--surface-raised))]
                  transition-colors
                "
              >
                <TagBadge tag={tag} size="sm" />
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
