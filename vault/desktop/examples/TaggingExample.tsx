/**
 * Auto-Tagging System Usage Examples
 *
 * This file demonstrates how to use the auto-tagging system
 * in your components.
 */

import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { TagManager, TagFilter, type Tag } from '../components';

// ============================================================================
// Example 1: Simple Tag Manager in Document View
// ============================================================================

export function DocumentViewWithTags({ documentId }: { documentId: string }) {
  return (
    <div className="document-view">
      <h1>Document Viewer</h1>

      {/* Document content */}
      <div className="content">
        {/* ... document content here ... */}
      </div>

      {/* Tag Management Section */}
      <div className="mt-6 border-t pt-4">
        <TagManager documentId={documentId} />
      </div>
    </div>
  );
}

// ============================================================================
// Example 2: Search with Tag Filtering
// ============================================================================

export function SearchWithTagFilter() {
  const [selectedTag, setSelectedTag] = useState<Tag | null>(null);
  const [searchResults, setSearchResults] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const handleTagSelect = async (tag: Tag | null) => {
    setSelectedTag(tag);

    if (tag) {
      // Filter by tag
      setIsLoading(true);
      try {
        const documentIds = await invoke<string[]>('search_by_tag', {
          tagName: tag.name
        });
        setSearchResults(documentIds);
      } catch (error) {
        console.error('Failed to filter by tag:', error);
      } finally {
        setIsLoading(false);
      }
    } else {
      // Clear filter
      setSearchResults([]);
    }
  };

  return (
    <div className="search-view">
      <h1>Search Documents</h1>

      {/* Tag Filter */}
      <div className="mb-4">
        <TagFilter
          onTagSelect={handleTagSelect}
          selectedTag={selectedTag}
        />
      </div>

      {/* Search Results */}
      <div className="results">
        {isLoading ? (
          <div>Loading...</div>
        ) : searchResults.length > 0 ? (
          <div>
            <p>Found {searchResults.length} documents</p>
            {/* Render document list */}
          </div>
        ) : selectedTag ? (
          <div>No documents with tag "{selectedTag.name}"</div>
        ) : (
          <div>Select a tag to filter</div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Example 3: Programmatic Tag Generation
// ============================================================================

export function AutoTagButton({ documentId }: { documentId: string }) {
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleAutoTag = async () => {
    setIsGenerating(true);
    setError(null);

    try {
      // Step 1: Generate tags using LLM
      const suggestedTags = await invoke<string[]>('generate_tags', {
        documentId
      });

      console.log('Generated tags:', suggestedTags);

      // Step 2: Apply tags to document
      const appliedTags = await invoke<Tag[]>('apply_tags', {
        documentId,
        tags: suggestedTags
      });

      console.log('Applied tags:', appliedTags);

      // Success!
      alert(`Successfully tagged with: ${appliedTags.map(t => t.name).join(', ')}`);
    } catch (err) {
      console.error('Auto-tag failed:', err);
      setError('Failed to auto-tag document');
    } finally {
      setIsGenerating(false);
    }
  };

  return (
    <div>
      <button
        onClick={handleAutoTag}
        disabled={isGenerating}
        className="px-4 py-2 bg-blue-600 text-white rounded disabled:opacity-50"
      >
        {isGenerating ? 'Generating...' : 'Auto-generate tags'}
      </button>

      {error && (
        <div className="mt-2 text-red-600">{error}</div>
      )}
    </div>
  );
}

// ============================================================================
// Example 4: Manual Tag Management
// ============================================================================

export function ManualTagManagement({ documentId }: { documentId: string }) {
  const [tags, setTags] = useState<Tag[]>([]);
  const [tagInput, setTagInput] = useState('');

  // Load tags
  useEffect(() => {
    loadTags();
  }, [documentId]);

  const loadTags = async () => {
    try {
      const documentTags = await invoke<Tag[]>('get_document_tags', {
        documentId
      });
      setTags(documentTags);
    } catch (error) {
      console.error('Failed to load tags:', error);
    }
  };

  // Add tag
  const handleAddTag = async () => {
    if (!tagInput.trim()) return;

    try {
      const appliedTags = await invoke<Tag[]>('apply_tags', {
        documentId,
        tags: [tagInput.trim().toLowerCase()]
      });
      setTags(appliedTags);
      setTagInput('');
    } catch (error) {
      console.error('Failed to add tag:', error);
    }
  };

  // Remove tag
  const handleRemoveTag = async (tagId: string) => {
    try {
      await invoke('remove_tag_from_document', {
        documentId,
        tagId
      });
      setTags(tags.filter(t => t.id !== tagId));
    } catch (error) {
      console.error('Failed to remove tag:', error);
    }
  };

  return (
    <div>
      <h3>Tags</h3>

      {/* Display tags */}
      <div className="flex gap-2 mb-2">
        {tags.map(tag => (
          <span
            key={tag.id}
            className="px-2 py-1 rounded"
            style={{ backgroundColor: tag.color + '20', color: tag.color }}
          >
            {tag.name}
            <button
              onClick={() => handleRemoveTag(tag.id)}
              className="ml-2"
            >
              ×
            </button>
          </span>
        ))}
      </div>

      {/* Add tag */}
      <div className="flex gap-2">
        <input
          type="text"
          value={tagInput}
          onChange={(e) => setTagInput(e.target.value)}
          onKeyPress={(e) => e.key === 'Enter' && handleAddTag()}
          placeholder="Add tag..."
          className="px-2 py-1 border rounded"
        />
        <button
          onClick={handleAddTag}
          className="px-3 py-1 bg-gray-200 rounded"
        >
          Add
        </button>
      </div>
    </div>
  );
}

// ============================================================================
// Example 5: Bulk Auto-Tagging
// ============================================================================

export function BulkAutoTagButton() {
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState<{ current: number; total: number } | null>(null);

  const handleBulkAutoTag = async () => {
    if (!confirm('This will auto-tag all untagged documents. Continue?')) {
      return;
    }

    setIsProcessing(true);
    setProgress({ current: 0, total: 0 });

    try {
      const taggedCount = await invoke<number>('auto_tag_all_documents');

      alert(`Successfully tagged ${taggedCount} documents!`);
    } catch (error) {
      console.error('Bulk auto-tag failed:', error);
      alert('Failed to auto-tag documents');
    } finally {
      setIsProcessing(false);
      setProgress(null);
    }
  };

  return (
    <div className="bulk-auto-tag">
      <button
        onClick={handleBulkAutoTag}
        disabled={isProcessing}
        className="px-4 py-2 bg-indigo-600 text-white rounded disabled:opacity-50"
      >
        {isProcessing ? 'Processing...' : 'Auto-tag all documents'}
      </button>

      {progress && (
        <div className="mt-2 text-sm text-gray-600">
          Tagged {progress.current} of {progress.total} documents...
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Example 6: Tag Statistics Dashboard
// ============================================================================

interface TagWithCount {
  id: string;
  name: string;
  color: string;
  documentCount: number;
}

export function TagStatistics() {
  const [tags, setTags] = useState<TagWithCount[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    loadTagStats();
  }, []);

  const loadTagStats = async () => {
    try {
      const tagStats = await invoke<TagWithCount[]>('get_all_tags_with_counts');
      setTags(tagStats.sort((a, b) => b.documentCount - a.documentCount));
    } catch (error) {
      console.error('Failed to load tag statistics:', error);
    } finally {
      setIsLoading(false);
    }
  };

  if (isLoading) {
    return <div>Loading tag statistics...</div>;
  }

  return (
    <div className="tag-statistics">
      <h2>Tag Statistics</h2>

      <div className="mt-4">
        <p className="text-sm text-gray-600">
          Total tags: {tags.length}
        </p>

        <div className="mt-4 space-y-2">
          {tags.map(tag => (
            <div
              key={tag.id}
              className="flex items-center justify-between p-2 border rounded"
            >
              <div className="flex items-center gap-2">
                <span
                  className="px-2 py-1 rounded text-sm"
                  style={{ backgroundColor: tag.color + '20', color: tag.color }}
                >
                  {tag.name}
                </span>
              </div>
              <div className="text-sm text-gray-600">
                {tag.documentCount} {tag.documentCount === 1 ? 'document' : 'documents'}
              </div>
            </div>
          ))}
        </div>

        {tags.length === 0 && (
          <div className="text-center text-gray-500 py-8">
            No tags yet. Start tagging documents!
          </div>
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Example 7: Advanced Tag Management with CRUD
// ============================================================================

export function AdvancedTagManagement() {
  const [tags, setTags] = useState<Tag[]>([]);
  const [editingTag, setEditingTag] = useState<Tag | null>(null);

  useEffect(() => {
    loadTags();
  }, []);

  const loadTags = async () => {
    const allTags = await invoke<Tag[]>('get_all_tags');
    setTags(allTags);
  };

  const handleCreateTag = async (name: string, color: string) => {
    try {
      const newTag = await invoke<Tag>('create_tag', { name, color });
      setTags([...tags, newTag]);
    } catch (error) {
      console.error('Failed to create tag:', error);
    }
  };

  const handleUpdateTag = async (tagId: string, name?: string, color?: string) => {
    try {
      const updatedTag = await invoke<Tag>('update_tag', {
        tagId,
        name,
        color
      });

      setTags(tags.map(t => t.id === tagId ? updatedTag : t));
      setEditingTag(null);
    } catch (error) {
      console.error('Failed to update tag:', error);
    }
  };

  const handleDeleteTag = async (tagId: string) => {
    if (!confirm('Delete this tag from all documents?')) return;

    try {
      await invoke('delete_tag', { tagId });
      setTags(tags.filter(t => t.id !== tagId));
    } catch (error) {
      console.error('Failed to delete tag:', error);
    }
  };

  return (
    <div className="advanced-tag-management">
      <h2>Manage Tags</h2>

      {/* Tag list with edit/delete */}
      <div className="mt-4 space-y-2">
        {tags.map(tag => (
          <div key={tag.id} className="flex items-center justify-between p-2 border rounded">
            {editingTag?.id === tag.id ? (
              // Edit mode
              <div className="flex gap-2">
                <input
                  type="text"
                  defaultValue={tag.name}
                  id={`name-${tag.id}`}
                  className="px-2 py-1 border rounded"
                />
                <input
                  type="color"
                  defaultValue={tag.color}
                  id={`color-${tag.id}`}
                  className="w-12"
                />
                <button
                  onClick={() => {
                    const name = (document.getElementById(`name-${tag.id}`) as HTMLInputElement).value;
                    const color = (document.getElementById(`color-${tag.id}`) as HTMLInputElement).value;
                    handleUpdateTag(tag.id, name, color);
                  }}
                  className="px-2 py-1 bg-green-600 text-white rounded text-sm"
                >
                  Save
                </button>
                <button
                  onClick={() => setEditingTag(null)}
                  className="px-2 py-1 bg-gray-300 rounded text-sm"
                >
                  Cancel
                </button>
              </div>
            ) : (
              // View mode
              <>
                <span
                  className="px-2 py-1 rounded"
                  style={{ backgroundColor: tag.color + '20', color: tag.color }}
                >
                  {tag.name}
                </span>
                <div className="flex gap-2">
                  <button
                    onClick={() => setEditingTag(tag)}
                    className="px-2 py-1 text-sm text-blue-600"
                  >
                    Edit
                  </button>
                  <button
                    onClick={() => handleDeleteTag(tag.id)}
                    className="px-2 py-1 text-sm text-red-600"
                  >
                    Delete
                  </button>
                </div>
              </>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
