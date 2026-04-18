import { useState, useCallback, useEffect} from 'react';

import VaultAPI from '../../lib/api';
import { DocumentViewer } from '../DocumentViewer';
import { QueryRewritePanel } from '../QueryRewritePanel';
import { ResultsList } from '../ResultsList';
import { SearchBar } from '../SearchBar';

import type { SearchResult } from '../../types';

/**
 * SearchView
 *
 * Purpose: Main search interface combining SearchBar and ResultsList
 *
 * Features:
 * - Integrated search state management
 * - Error handling with user-friendly messages
 * - Recent searches tracking (future enhancement)
 * - Search analytics (future enhancement)
 *
 * This component orchestrates the search experience:
 * 1. User enters query in SearchBar
 * 2. Query is sent to backend via VaultAPI
 * 3. Results are displayed in ResultsList
 * 4. User can click results to open documents
 *
 * States: idle, searching, results, error
 * Accessibility: WCAG AA, keyboard navigation, screen reader friendly
 * Performance: Optimized with useCallback to prevent child re-renders
 */

export function SearchView() {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasSearched, setHasSearched] = useState(false);
  const [currentQuery, setCurrentQuery] = useState('');
  const [currentMode, setCurrentMode] = useState<'semantic' | 'keyword' | 'hybrid'>('hybrid');
  const [showRewritePanel, setShowRewritePanel] = useState(false);
  const [viewingDocument, setViewingDocument] = useState<SearchResult | null>(null);

  // Handle search execution
  const handleSearch = useCallback(async (query: string, mode: 'semantic' | 'keyword' | 'hybrid') => {
    // Clear results if query is empty
    if (!query.trim()) {
      setResults([]);
      setHasSearched(false);
      setError(null);
      setCurrentQuery('');
      setShowRewritePanel(false);
      return;
    }

    setCurrentQuery(query);
    setCurrentMode(mode);
    setIsSearching(true);
    setError(null);
    setHasSearched(true);

    try {
      const result = await VaultAPI.searchHybrid(query, 20, mode);

      // Null/undefined check - handle service unavailability
      if (!result) {
        throw new Error('Search service is unavailable. Please check if the backend is running.');
      }

      if (result.ok && result.data) {
        setResults(result.data);
        setError(null);

        // Auto-show rewrite panel if no results found
        if (result.data.length === 0) {
          setShowRewritePanel(true);
        }
      } else {
        const errorMessage = (result as { ok: false; error: string }).error || 'Search failed with no error message. Please try again.';
        setError(errorMessage);
        setResults([]);
        console.error('Search error:', errorMessage);
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'An unexpected error occurred';
      setError(errorMessage);
      setResults([]);
      console.error('Search error:', err);
    } finally {
      setIsSearching(false);
    }
  }, []);

  // Handle result click - open document viewer
  const handleResultClick = useCallback((result: SearchResult) => {
    setViewingDocument(result);
  }, []);

  // Handle document viewer navigation
  const handleDocumentNavigate = useCallback((result: SearchResult) => {
    setViewingDocument(result);
  }, []);

  // Handle document viewer close
  const handleDocumentClose = useCallback(() => {
    setViewingDocument(null);
  }, []);

  // Handle variant selection from QueryRewritePanel
  const handleVariantSelect = useCallback(
    (query: string) => {
      setShowRewritePanel(false);
      handleSearch(query, currentMode);
    },
    [currentMode, handleSearch]
  );

  // Toggle rewrite panel
  const toggleRewritePanel = useCallback(() => {
    setShowRewritePanel((prev) => !prev);
  }, []);

  // Keyboard shortcut for query rewrites (Cmd/Ctrl + R)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'r' && currentQuery) {
        e.preventDefault();
        toggleRewritePanel();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [currentQuery, toggleRewritePanel]);

  return (
    <div className="flex-1 flex flex-col h-full bg-[hsl(var(--surface))]">
      {/* Search Header */}
      <div className="bg-[hsl(var(--surface-raised))] border-b border-[hsl(var(--border-subtle))] px-6 py-4">
        <div className="space-y-4">
          <div className="flex items-center gap-3">
            <div className="flex-1">
              <SearchBar
                onSearch={handleSearch}
                isSearching={isSearching}
              />
            </div>
            {/* Suggest Rewrites Button */}
            {currentQuery && (
              <button
                onClick={toggleRewritePanel}
                className={`
                  px-4 py-2 text-sm font-medium rounded-md
                  transition-colors duration-fast
                  focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2
                  ${
                    showRewritePanel
                      ? 'bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] shadow-sm'
                      : 'bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))] border border-[hsl(var(--border-subtle))] hover:bg-[hsl(var(--surface))]'
                  }
                `}
                aria-label="Toggle query suggestions"
                title="Suggest alternative queries (Cmd/Ctrl + R)"
              >
                <div className="flex items-center gap-2">
                  <svg
                    className="w-4 h-4"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={1.75}
                      d="M13 10V3L4 14h7v7l9-11h-7z"
                    />
                  </svg>
                  <span>Suggest rewrites</span>
                </div>
              </button>
            )}
          </div>

          {/* Query Rewrite Panel */}
          {showRewritePanel && currentQuery && (
            <QueryRewritePanel
              originalQuery={currentQuery}
              onVariantSelect={handleVariantSelect}
              onClose={() => setShowRewritePanel(false)}
              isVisible={showRewritePanel}
              autoGenerate
            />
          )}
        </div>
      </div>

      {/* Results Area */}
      <div className="flex-1 overflow-auto px-6 py-6">
        {!hasSearched ? (
          <WelcomeMessage />
        ) : (
          <>
            {/* No Results Message with Suggestion */}
            {results.length === 0 && !isSearching && !error && (
              <div className="flex flex-col items-center justify-center py-12">
                <div className="w-16 h-16 bg-[hsl(var(--surface-raised))] rounded-full flex items-center justify-center mb-4">
                  <svg
                    className="w-8 h-8 text-[hsl(var(--text-tertiary))]"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                    />
                  </svg>
                </div>
                <h3 className="text-xl font-semibold text-[hsl(var(--text-primary))] mb-2">
                  No results found
                </h3>
                <p className="text-[hsl(var(--text-secondary))] mb-4 text-center max-w-md">
                  We couldn't find any documents matching "{currentQuery}". Try using different keywords or let AI suggest alternative queries.
                </p>
                {!showRewritePanel && (
                  <button
                    onClick={toggleRewritePanel}
                    className="px-4 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))]"
                  >
                    Suggest alternative queries
                  </button>
                )}
              </div>
            )}

            {/* Results List */}
            <ResultsList
              results={results}
              isLoading={isSearching}
              error={error}
              onResultClick={handleResultClick}
            />
          </>
        )}
      </div>

      {/* Document Viewer Modal */}
      {viewingDocument && (
        <DocumentViewer
          result={viewingDocument}
          searchResults={results}
          onClose={handleDocumentClose}
          onNavigate={handleDocumentNavigate}
        />
      )}
    </div>
  );
}

// Welcome message shown before first search
function WelcomeMessage() {
  return (
    <div className="flex flex-col items-center justify-center h-full text-center px-4">
      <div className="w-20 h-20 bg-[hsl(var(--accent))] rounded-lg flex items-center justify-center mb-6 shadow-sm">
        <svg
          className="w-10 h-10 text-[hsl(var(--accent-fg))]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.75}
            d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
          />
        </svg>
      </div>

      <h2 className="text-2xl font-bold text-[hsl(var(--text-primary))] mb-2">
        Search your documents
      </h2>

      <p className="text-[hsl(var(--text-secondary))] max-w-md mb-6">
        Start typing to search through your indexed documents using AI-powered semantic search,
        fast keyword matching, or a hybrid approach.
      </p>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 max-w-3xl">
        <FeatureCard
          icon={
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
          }
          title="Semantic Search"
          description="Find documents by meaning, not just keywords"
        />
        <FeatureCard
          icon={
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
            </svg>
          }
          title="Hybrid Mode"
          description="Combine semantic and keyword search for best results"
        />
        <FeatureCard
          icon={
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
          }
          title="Lightning Fast"
          description="Instant results from your local document index"
        />
      </div>
    </div>
  );
}

// Feature card component
interface FeatureCardProps {
  icon: React.ReactNode;
  title: string;
  description: string;
}

function FeatureCard({ icon, title, description }: FeatureCardProps) {
  return (
    <div className="bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg p-4 text-left">
      <div className="text-[hsl(var(--accent))] mb-2">{icon}</div>
      <h3 className="font-semibold text-[hsl(var(--text-primary))] mb-1">{title}</h3>
      <p className="text-sm text-[hsl(var(--text-secondary))]">{description}</p>
    </div>
  );
}
