/**
 * RecentDocuments Component
 *
 * Purpose: Quick access to recently viewed/edited documents
 *
 * Features:
 * - Shows last 10 accessed documents
 * - Timestamp display (relative time)
 * - Document type icons
 * - Keyboard navigation
 * - Persistent storage (localStorage)
 *
 * States: Loading, Empty, Error, Populated
 * Accessibility: Keyboard navigation, ARIA labels, semantic HTML
 */

import { useState, useEffect, useCallback } from 'react';

import { Clock, FileText, Calendar} from 'lucide-react';

import { NoRecentDocuments } from '../EmptyState';
import { LoadingState } from '../LoadingState';

interface RecentDocument {
  id: string;
  path: string;
  title: string;
  type: 'note' | 'document' | 'daily';
  lastAccessed: number; // timestamp
  excerpt?: string;
}

export interface RecentDocumentsProps {
  /** Maximum number of recent documents to show */
  maxItems?: number;

  /** Callback when document is clicked */
  onDocumentClick?: (doc: RecentDocument) => void;

  /** Show as compact list */
  compact?: boolean;

  /** Additional CSS classes */
  className?: string;
}

export function RecentDocuments({
  maxItems = 10,
  onDocumentClick,
  compact = false,
  className = '',
}: RecentDocumentsProps) {
  const [documents, setDocuments] = useState<RecentDocument[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Load recent documents from localStorage
  const loadRecentDocuments = useCallback(() => {
    setIsLoading(true);
    setError(null);

    try {
      const stored = localStorage.getItem('vault:recentDocuments');
      if (stored) {
        const parsed = JSON.parse(stored) as RecentDocument[];
        // Sort by last accessed (most recent first)
        const sorted = parsed
          .sort((a, b) => b.lastAccessed - a.lastAccessed)
          .slice(0, maxItems);
        setDocuments(sorted);
      } else {
        setDocuments([]);
      }
    } catch (err) {
      setError('Failed to load recent documents');
      console.error('Error loading recent documents:', err);
    } finally {
      setIsLoading(false);
    }
  }, [maxItems]);

  useEffect(() => {
    loadRecentDocuments();
  }, [loadRecentDocuments]);

  const handleDocumentClick = (doc: RecentDocument) => {
    // Update last accessed time
    const updated = documents.map((d) =>
      d.id === doc.id ? { ...d, lastAccessed: Date.now() } : d
    );

    // Save to localStorage
    try {
      localStorage.setItem('vault:recentDocuments', JSON.stringify(updated));
      setDocuments(updated.sort((a, b) => b.lastAccessed - a.lastAccessed));
    } catch (err) {
      console.error('Error updating recent documents:', err);
    }

    // Callback
    onDocumentClick?.(doc);
  };

  const handleClearAll = () => {
    try {
      localStorage.removeItem('vault:recentDocuments');
      setDocuments([]);
    } catch (err) {
      console.error('Error clearing recent documents:', err);
    }
  };

  if (isLoading) {
    return (
      <div className={className}>
        <LoadingState type="dots" size="sm" message="Loading recent documents..." centered={false} />
      </div>
    );
  }

  if (error) {
    return (
      <div className={`p-4 bg-[hsl(var(--danger-muted))]/20 border border-[hsl(var(--danger-muted))] rounded-lg ${className}`}>
        <p className="text-sm text-[hsl(var(--danger-fg))]">{error}</p>
      </div>
    );
  }

  if (documents.length === 0) {
    return (
      <div className={className}>
        <NoRecentDocuments />
      </div>
    );
  }

  return (
    <div className={className}>
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Clock className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
          <h3 className="text-sm font-semibold text-[hsl(var(--text-primary))]">
            Recent Documents
          </h3>
        </div>
        <button
          onClick={handleClearAll}
          className="text-xs text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-secondary))] transition-colors"
        >
          Clear all
        </button>
      </div>

      {/* Document List */}
      <div className={`space-y-${compact ? '1' : '2'}`}>
        {documents.map((doc) => (
          <DocumentItem
            key={doc.id}
            document={doc}
            onClick={() => handleDocumentClick(doc)}
            compact={compact}
          />
        ))}
      </div>
    </div>
  );
}

interface DocumentItemProps {
  document: RecentDocument;
  onClick: () => void;
  compact: boolean;
}

function DocumentItem({ document, onClick, compact }: DocumentItemProps) {
  const getDocumentIcon = (type: RecentDocument['type']) => {
    switch (type) {
      case 'daily':
        return <Calendar className="w-4 h-4" />;
      case 'note':
        return <FileText className="w-4 h-4" />;
      case 'document':
        return <FileText className="w-4 h-4" />;
      default:
        return <FileText className="w-4 h-4" />;
    }
  };

  const formatRelativeTime = (timestamp: number) => {
    const now = Date.now();
    const diff = now - timestamp;
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) return `${days}d ago`;
    if (hours > 0) return `${hours}h ago`;
    if (minutes > 0) return `${minutes}m ago`;
    return 'Just now';
  };

  if (compact) {
    return (
      <button
        onClick={onClick}
        className="w-full flex items-center gap-2 px-3 py-2 text-left rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors group"
      >
        <div className="text-[hsl(var(--text-tertiary))] group-hover:text-[hsl(var(--text-secondary))]">
          {getDocumentIcon(document.type)}
        </div>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-[hsl(var(--text-primary))] truncate">
            {document.title}
          </p>
        </div>
        <span className="text-xs text-[hsl(var(--text-secondary))] whitespace-nowrap">
          {formatRelativeTime(document.lastAccessed)}
        </span>
      </button>
    );
  }

  return (
    <button
      onClick={onClick}
      className="w-full flex items-start gap-3 p-3 text-left bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg hover:border-[hsl(var(--border-subtle))] hover:shadow-sm transition-all group"
    >
      <div className="mt-1 text-[hsl(var(--text-tertiary))] group-hover:text-[hsl(var(--text-secondary))]">
        {getDocumentIcon(document.type)}
      </div>
      <div className="flex-1 min-w-0">
        <h4 className="text-sm font-medium text-[hsl(var(--text-primary))] truncate mb-1">
          {document.title}
        </h4>
        {document.excerpt && (
          <p className="text-xs text-[hsl(var(--text-secondary))] line-clamp-2 mb-2">
            {document.excerpt}
          </p>
        )}
        <div className="flex items-center gap-2 text-xs text-[hsl(var(--text-secondary))]">
          <span>{formatRelativeTime(document.lastAccessed)}</span>
          <span>•</span>
          <span className="truncate">{document.path}</span>
        </div>
      </div>
    </button>
  );
}

/**
 * Utility function to add document to recent list
 * Call this when a document is opened/viewed
 */
export function addToRecentDocuments(doc: Omit<RecentDocument, 'lastAccessed'>) {
  try {
    const stored = localStorage.getItem('vault:recentDocuments');
    let documents: RecentDocument[] = stored ? JSON.parse(stored) : [];

    // Remove existing entry if present
    documents = documents.filter((d) => d.id !== doc.id);

    // Add new entry
    documents.unshift({
      ...doc,
      lastAccessed: Date.now(),
    });

    // Keep only last 50 (store more than we show for history)
    documents = documents.slice(0, 50);

    // Save
    localStorage.setItem('vault:recentDocuments', JSON.stringify(documents));
  } catch (err) {
    console.error('Error adding to recent documents:', err);
  }
}

/**
 * Utility function to clear all recent documents
 */
export function clearRecentDocuments() {
  try {
    localStorage.removeItem('vault:recentDocuments');
  } catch (err) {
    console.error('Error clearing recent documents:', err);
  }
}

RecentDocuments.displayName = 'RecentDocuments';
