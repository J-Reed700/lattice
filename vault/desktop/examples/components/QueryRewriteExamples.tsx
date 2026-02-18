/**
 * Query Rewriting UI - Example Usage
 *
 * This file demonstrates various ways to use the QueryRewritePanel component
 * in different scenarios and configurations.
 */

import React, { useState } from 'react';
import { QueryRewritePanel } from './QueryRewritePanel';
import type { QueryVariant } from './QueryRewritePanel';

// ============================================================================
// Example 1: Basic Usage
// ============================================================================

export function BasicExample() {
  const [query, setQuery] = useState('machine learning');
  const [showPanel, setShowPanel] = useState(true);

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-2xl font-bold">Basic Usage</h2>

      <button
        onClick={() => setShowPanel(!showPanel)}
        className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg"
      >
        Toggle Query Suggestions
      </button>

      {showPanel && (
        <QueryRewritePanel
          originalQuery={query}
          onVariantSelect={(selectedQuery) => {
            console.log('User selected:', selectedQuery);
            setQuery(selectedQuery);
            setShowPanel(false);
          }}
          onClose={() => setShowPanel(false)}
          isVisible={showPanel}
          autoGenerate={true}
        />
      )}
    </div>
  );
}

// ============================================================================
// Example 2: Integrated with Search
// ============================================================================

export function IntegratedSearchExample() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<string[]>([]);
  const [showRewrites, setShowRewrites] = useState(false);

  const handleSearch = (searchQuery: string) => {
    console.log('Searching for:', searchQuery);
    // Simulate search
    setResults([
      `Result 1 for "${searchQuery}"`,
      `Result 2 for "${searchQuery}"`,
      `Result 3 for "${searchQuery}"`,
    ]);
    setQuery(searchQuery);
  };

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-2xl font-bold">Integrated Search Example</h2>

      {/* Search Bar */}
      <div className="flex gap-2">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              handleSearch(query);
            }
          }}
          placeholder="Enter search query..."
          className="flex-1 px-4 py-2 border border-[var(--border-color)] rounded-lg"
        />
        <button
          onClick={() => handleSearch(query)}
          className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg"
        >
          Search
        </button>
        {query && (
          <button
            onClick={() => setShowRewrites(!showRewrites)}
            className="px-4 py-2 bg-[var(--bg-tertiary)] text-[var(--text-secondary)] rounded-lg"
          >
            Suggest Rewrites
          </button>
        )}
      </div>

      {/* Query Rewrite Panel */}
      {showRewrites && query && (
        <QueryRewritePanel
          originalQuery={query}
          onVariantSelect={(variant) => {
            setShowRewrites(false);
            handleSearch(variant);
          }}
          onClose={() => setShowRewrites(false)}
          isVisible={showRewrites}
          autoGenerate={true}
        />
      )}

      {/* Results */}
      {results.length > 0 && (
        <div className="space-y-2">
          <h3 className="font-semibold">Results:</h3>
          {results.map((result, idx) => (
            <div key={idx} className="p-3 bg-[var(--bg-tertiary)] rounded-lg">
              {result}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Example 3: Auto-Show on No Results
// ============================================================================

export function AutoShowExample() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<string[]>([]);
  const [showRewrites, setShowRewrites] = useState(false);
  const [isSearching, setIsSearching] = useState(false);

  const handleSearch = async (searchQuery: string) => {
    setIsSearching(true);
    setQuery(searchQuery);

    // Simulate async search
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Simulate no results scenario
    const searchResults: string[] = [];
    setResults(searchResults);
    setIsSearching(false);

    // Auto-show rewrites if no results
    if (searchResults.length === 0 && searchQuery.trim()) {
      setShowRewrites(true);
    }
  };

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-2xl font-bold">Auto-Show on No Results</h2>

      <div className="flex gap-2">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Try searching for something obscure..."
          className="flex-1 px-4 py-2 border border-[var(--border-color)] rounded-lg"
        />
        <button
          onClick={() => handleSearch(query)}
          disabled={isSearching}
          className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg disabled:opacity-50"
        >
          {isSearching ? 'Searching...' : 'Search'}
        </button>
      </div>

      {/* No Results Message */}
      {!isSearching && results.length === 0 && query && !showRewrites && (
        <div className="p-6 text-center bg-[var(--bg-tertiary)] rounded-lg">
          <p className="text-[var(--text-secondary)] mb-3">No results found for "{query}"</p>
          <button
            onClick={() => setShowRewrites(true)}
            className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg"
          >
            Suggest Alternative Queries
          </button>
        </div>
      )}

      {/* Query Rewrite Panel */}
      {showRewrites && (
        <QueryRewritePanel
          originalQuery={query}
          onVariantSelect={(variant) => {
            setShowRewrites(false);
            handleSearch(variant);
          }}
          onClose={() => setShowRewrites(false)}
          isVisible={showRewrites}
          autoGenerate={true}
        />
      )}
    </div>
  );
}

// ============================================================================
// Example 4: Manual Control (No Auto-Generate)
// ============================================================================

export function ManualControlExample() {
  const [query, setQuery] = useState('artificial intelligence');
  const [showPanel, setShowPanel] = useState(false);

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-2xl font-bold">Manual Control Example</h2>

      <p className="text-[var(--text-secondary)]">
        In this example, the panel doesn't auto-generate. You control when generation starts.
      </p>

      <button
        onClick={() => setShowPanel(!showPanel)}
        className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg"
      >
        {showPanel ? 'Hide' : 'Show'} Panel
      </button>

      {showPanel && (
        <QueryRewritePanel
          originalQuery={query}
          onVariantSelect={(selectedQuery) => {
            console.log('Selected:', selectedQuery);
            setQuery(selectedQuery);
          }}
          onClose={() => setShowPanel(false)}
          isVisible={showPanel}
          autoGenerate={false} // Manual control
        />
      )}

      <div className="p-4 bg-[var(--bg-tertiary)] rounded-lg">
        <p className="text-sm text-[var(--text-secondary)]">
          Current Query: <strong>{query}</strong>
        </p>
        <p className="text-xs text-[var(--text-tertiary)] mt-2">
          Click "Regenerate" button in the panel to start generation manually.
        </p>
      </div>
    </div>
  );
}

// ============================================================================
// Example 5: Custom Styling
// ============================================================================

export function CustomStylingExample() {
  const [query, setQuery] = useState('react hooks');
  const [showPanel, setShowPanel] = useState(true);

  return (
    <div className="p-6 space-y-4 bg-gradient-to-br from-purple-100 to-pink-100 rounded-xl">
      <h2 className="text-2xl font-bold text-purple-900">Custom Styling Example</h2>

      <div className="bg-[var(--surface-elevated)] rounded-lg shadow-lg p-4">
        {showPanel && (
          <QueryRewritePanel
            originalQuery={query}
            onVariantSelect={(selectedQuery) => {
              console.log('Selected:', selectedQuery);
              setQuery(selectedQuery);
            }}
            onClose={() => setShowPanel(false)}
            isVisible={showPanel}
            autoGenerate={false}
          />
        )}
      </div>

      <p className="text-sm text-purple-700">
        The panel inherits parent container styling and adapts to dark/light modes automatically.
      </p>
    </div>
  );
}

// ============================================================================
// Example 6: Error Handling
// ============================================================================

export function ErrorHandlingExample() {
  const [query, setQuery] = useState('test query');
  const [showPanel, setShowPanel] = useState(true);

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-2xl font-bold">Error Handling Example</h2>

      <div className="p-4 bg-[var(--warning-light)] border border-[var(--warning-light)] rounded-lg">
        <p className="text-sm text-[var(--warning)]">
          <strong>Testing Error States:</strong>
          <br />
          To test error handling, make sure Ollama is NOT running before clicking the button below.
        </p>
      </div>

      <button
        onClick={() => setShowPanel(true)}
        className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg"
      >
        Trigger Query Rewriting (with potential error)
      </button>

      {showPanel && (
        <QueryRewritePanel
          originalQuery={query}
          onVariantSelect={(selectedQuery) => {
            console.log('Selected:', selectedQuery);
            setQuery(selectedQuery);
            setShowPanel(false);
          }}
          onClose={() => setShowPanel(false)}
          isVisible={showPanel}
          autoGenerate={true}
        />
      )}

      <div className="p-4 bg-[var(--bg-tertiary)] rounded-lg">
        <h3 className="font-semibold mb-2">Expected Error Message:</h3>
        <ul className="text-sm text-[var(--text-secondary)] list-disc list-inside space-y-1">
          <li>"Failed to generate suggestions"</li>
          <li>"Make sure Ollama is running and a model is loaded"</li>
          <li>Red error card with retry option</li>
        </ul>
      </div>
    </div>
  );
}

// ============================================================================
// Example 7: Programmatic Variant Selection
// ============================================================================

export function ProgrammaticSelectionExample() {
  const [query, setQuery] = useState('python programming');
  const [showPanel, setShowPanel] = useState(true);
  const [selectedVariant, setSelectedVariant] = useState<string | null>(null);

  const handleVariantSelect = (variant: string) => {
    setSelectedVariant(variant);
    setQuery(variant);
    console.log('Variant selected:', variant);
    // Here you would trigger actual search
  };

  return (
    <div className="p-6 space-y-4">
      <h2 className="text-2xl font-bold">Programmatic Selection Example</h2>

      <div className="space-y-2">
        <p className="text-[var(--text-secondary)]">Use keyboard shortcuts to select variants:</p>
        <div className="flex gap-2 text-sm">
          <kbd className="px-2 py-1 bg-[var(--bg-tertiary)] rounded">0</kbd>
          <span>Original</span>
          <kbd className="px-2 py-1 bg-[var(--bg-tertiary)] rounded">1-3</kbd>
          <span>Variants</span>
          <kbd className="px-2 py-1 bg-[var(--bg-tertiary)] rounded">Esc</kbd>
          <span>Close</span>
        </div>
      </div>

      {showPanel && (
        <QueryRewritePanel
          originalQuery={query}
          onVariantSelect={handleVariantSelect}
          onClose={() => setShowPanel(false)}
          isVisible={showPanel}
          autoGenerate={false}
        />
      )}

      {selectedVariant && (
        <div className="p-4 bg-[var(--success-light)] border border-[var(--success-light)] rounded-lg">
          <p className="text-sm text-[var(--success)]">
            <strong>Selected Variant:</strong> {selectedVariant}
          </p>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Example 8: Complete Demo
// ============================================================================

export function CompleteDemoExample() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Array<{ id: string; title: string; score: number }>>([]);
  const [showRewrites, setShowRewrites] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [searchHistory, setSearchHistory] = useState<string[]>([]);

  const performSearch = async (searchQuery: string) => {
    if (!searchQuery.trim()) return;

    setIsSearching(true);
    setQuery(searchQuery);
    setShowRewrites(false);

    // Add to history
    setSearchHistory((prev) => [searchQuery, ...prev.slice(0, 4)]);

    // Simulate search with delay
    await new Promise((resolve) => setTimeout(resolve, 800));

    // Simulate variable results
    const resultCount = Math.random() > 0.3 ? Math.floor(Math.random() * 10) : 0;

    const searchResults = Array.from({ length: resultCount }, (_, i) => ({
      id: `result-${i}`,
      title: `Document ${i + 1} matching "${searchQuery}"`,
      score: Math.random() * 100,
    }));

    setResults(searchResults);
    setIsSearching(false);

    // Auto-show rewrites if no results
    if (searchResults.length === 0) {
      setShowRewrites(true);
    }
  };

  return (
    <div className="p-6 space-y-4 max-w-4xl mx-auto">
      <h2 className="text-3xl font-bold mb-6">Complete Query Rewriting Demo</h2>

      {/* Search Interface */}
      <div className="bg-[var(--surface-elevated)] rounded-lg shadow-lg p-6">
        <div className="flex gap-2 mb-4">
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                performSearch(query);
              }
              if ((e.metaKey || e.ctrlKey) && e.key === 'r' && query) {
                e.preventDefault();
                setShowRewrites(!showRewrites);
              }
            }}
            placeholder="Search your documents... (Cmd/Ctrl+R for suggestions)"
            className="flex-1 px-4 py-3 border border-[var(--border-color)] rounded-lg focus:ring-2 ring-[var(--accent-primary)]"
          />
          <button
            onClick={() => performSearch(query)}
            disabled={isSearching || !query.trim()}
            className="px-6 py-3 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isSearching ? 'Searching...' : 'Search'}
          </button>
          {query && (
            <button
              onClick={() => setShowRewrites(!showRewrites)}
              className={`px-4 py-3 rounded-lg transition-colors ${
                showRewrites
                  ? 'bg-[var(--accent-primary)] text-white'
                  : 'bg-[var(--bg-tertiary)] text-[var(--text-secondary)] hover:bg-gray-300'
              }`}
              title="Suggest alternative queries (Cmd/Ctrl+R)"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M13 10V3L4 14h7v7l9-11h-7z"
                />
              </svg>
            </button>
          )}
        </div>

        {/* Query Rewrite Panel */}
        {showRewrites && query && (
          <div className="mb-4">
            <QueryRewritePanel
              originalQuery={query}
              onVariantSelect={(variant) => {
                setShowRewrites(false);
                performSearch(variant);
              }}
              onClose={() => setShowRewrites(false)}
              isVisible={showRewrites}
              autoGenerate={true}
            />
          </div>
        )}

        {/* Search History */}
        {searchHistory.length > 0 && !query && (
          <div className="mt-4">
            <h3 className="text-sm font-semibold text-[var(--text-secondary)] mb-2">
              Recent Searches
            </h3>
            <div className="flex flex-wrap gap-2">
              {searchHistory.map((historyQuery, idx) => (
                <button
                  key={idx}
                  onClick={() => performSearch(historyQuery)}
                  className="px-3 py-1 text-sm bg-[var(--bg-tertiary)] text-[var(--text-secondary)] rounded-full hover:bg-[var(--surface-hover)] transition-colors"
                >
                  {historyQuery}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Results Area */}
      {query && (
        <div className="bg-[var(--surface-elevated)] rounded-lg shadow-lg p-6">
          {isSearching ? (
            <div className="text-center py-12">
              <div className="animate-spin h-8 w-8 border-4 border-[var(--accent-primary)] border-t-transparent rounded-full mx-auto mb-4"></div>
              <p className="text-[var(--text-secondary)]">Searching...</p>
            </div>
          ) : results.length > 0 ? (
            <div>
              <h3 className="font-semibold mb-4">
                Found {results.length} results for "{query}"
              </h3>
              <div className="space-y-3">
                {results.map((result) => (
                  <div
                    key={result.id}
                    className="p-4 border border-[var(--border-color)] rounded-lg hover:shadow-md transition-shadow cursor-pointer"
                  >
                    <h4 className="font-medium text-[var(--text-primary)]">
                      {result.title}
                    </h4>
                    <p className="text-sm text-[var(--text-secondary)] mt-1">
                      Relevance: {result.score.toFixed(1)}%
                    </p>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="text-center py-12">
              <div className="w-16 h-16 bg-[var(--bg-tertiary)] rounded-full flex items-center justify-center mx-auto mb-4">
                <svg
                  className="w-8 h-8 text-[var(--text-tertiary)]"
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
              <h3 className="text-xl font-semibold text-[var(--text-primary)] mb-2">
                No results found
              </h3>
              <p className="text-[var(--text-secondary)] mb-4">
                We couldn't find any documents matching "{query}"
              </p>
              {!showRewrites && (
                <button
                  onClick={() => setShowRewrites(true)}
                  className="px-4 py-2 bg-[var(--accent-primary)] text-white rounded-lg hover:bg-[var(--accent-hover)] transition-colors"
                >
                  Suggest alternative queries
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Example Component Gallery
// ============================================================================

export function ExampleGallery() {
  const examples = [
    { name: 'Basic Usage', component: BasicExample },
    { name: 'Integrated Search', component: IntegratedSearchExample },
    { name: 'Auto-Show on No Results', component: AutoShowExample },
    { name: 'Manual Control', component: ManualControlExample },
    { name: 'Custom Styling', component: CustomStylingExample },
    { name: 'Error Handling', component: ErrorHandlingExample },
    { name: 'Programmatic Selection', component: ProgrammaticSelectionExample },
    { name: 'Complete Demo', component: CompleteDemoExample },
  ];

  const [selectedExample, setSelectedExample] = useState(0);
  const CurrentExample = examples[selectedExample].component;

  return (
    <div className="min-h-screen bg-[var(--bg-primary)] p-8">
      <div className="max-w-6xl mx-auto">
        <h1 className="text-4xl font-bold mb-8 text-[var(--text-primary)]">
          QueryRewritePanel Examples
        </h1>

        {/* Example Selector */}
        <div className="mb-8 flex flex-wrap gap-2">
          {examples.map((example, idx) => (
            <button
              key={idx}
              onClick={() => setSelectedExample(idx)}
              className={`px-4 py-2 rounded-lg transition-colors ${
                selectedExample === idx
                  ? 'bg-[var(--accent-primary)] text-white'
                  : 'bg-[var(--surface-elevated)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'
              }`}
            >
              {example.name}
            </button>
          ))}
        </div>

        {/* Current Example */}
        <div className="bg-[var(--surface-elevated)] rounded-lg shadow-xl p-8">
          <CurrentExample />
        </div>
      </div>
    </div>
  );
}

export default ExampleGallery;
