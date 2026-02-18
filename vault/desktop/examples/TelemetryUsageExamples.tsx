/**
 * Telemetry Usage Examples
 *
 * This file demonstrates how to integrate telemetry tracking
 * in React components for the Vault Desktop application.
 */

import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Telemetry, useTelemetry, withTelemetry } from '@/lib/telemetry';

// ============================================================================
// Example 1: Search Component with Telemetry
// ============================================================================

export function SearchView() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<any[]>([]);
  const [isSearching, setIsSearching] = useState(false);

  const handleSearch = async () => {
    if (!query.trim()) return;

    setIsSearching(true);
    const startTime = performance.now();

    try {
      const response = await invoke('search_documents', {
        options: {
          query,
          limit: 10,
          searchMode: 'hybrid',
        },
      });

      const elapsed = performance.now() - startTime;

      // Track successful search
      Telemetry.recordSearch(
        query,
        response.results.length,
        elapsed,
        'hybrid'
      );

      setResults(response.results);
    } catch (error) {
      // Track search error
      Telemetry.recordError(
        error as Error,
        'search_view',
        'error'
      );
      console.error('Search failed:', error);
    } finally {
      setIsSearching(false);
    }
  };

  return (
    <div className="search-view">
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyPress={(e) => e.key === 'Enter' && handleSearch()}
        placeholder="Search documents..."
      />
      <button onClick={handleSearch} disabled={isSearching}>
        {isSearching ? 'Searching...' : 'Search'}
      </button>

      <div className="results">
        {results.map((result) => (
          <SearchResultItem key={result.id} result={result} />
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// Example 2: Document View with Open Tracking
// ============================================================================

interface DocumentViewerProps {
  documentId: string;
  documentType: string;
}

export function DocumentViewer({ documentId, documentType }: DocumentViewerProps) {
  const [content, setContent] = useState<string>('');

  useEffect(() => {
    // Track document open
    Telemetry.recordDocumentOpen(documentId, documentType);

    // Load document content
    loadDocument();
  }, [documentId]);

  const loadDocument = async () => {
    try {
      const result = await invoke('read_file_content', {
        path: documentId,
      });
      setContent(result);
    } catch (error) {
      Telemetry.recordError(
        error as Error,
        'document_viewer',
        'error'
      );
    }
  };

  return (
    <div className="document-viewer">
      <h2>Document: {documentId}</h2>
      <pre>{content}</pre>
    </div>
  );
}

// ============================================================================
// Example 3: Indexing Progress with Telemetry
// ============================================================================

export function IndexingPanel() {
  const [isIndexing, setIsIndexing] = useState(false);
  const [progress, setProgress] = useState(0);

  const handleStartIndexing = async (path: string, recursive: boolean) => {
    setIsIndexing(true);
    const startTime = performance.now();

    try {
      await invoke('start_indexing', {
        path,
        recursive,
      });

      const elapsed = performance.now() - startTime;
      const stats = await invoke('get_indexing_stats');

      // Track indexing completion
      Telemetry.recordIndexing(
        path,
        stats.filesIndexed,
        elapsed,
        true
      );

      console.log('Indexing completed successfully');
    } catch (error) {
      const elapsed = performance.now() - startTime;

      // Track indexing failure
      Telemetry.recordIndexing(
        path,
        0,
        elapsed,
        false
      );

      Telemetry.recordError(
        error as Error,
        'indexing_panel',
        'error'
      );
    } finally {
      setIsIndexing(false);
    }
  };

  return (
    <div className="indexing-panel">
      <button
        onClick={() => handleStartIndexing('/path/to/docs', true)}
        disabled={isIndexing}
      >
        {isIndexing ? `Indexing... ${progress}%` : 'Start Indexing'}
      </button>
    </div>
  );
}

// ============================================================================
// Example 4: Q&A Interface with Telemetry
// ============================================================================

export function QAInterface() {
  const [question, setQuestion] = useState('');
  const [answer, setAnswer] = useState('');
  const [sources, setSources] = useState<any[]>([]);
  const [isAsking, setIsAsking] = useState(false);

  const handleAskQuestion = async () => {
    if (!question.trim()) return;

    setIsAsking(true);
    const startTime = performance.now();

    try {
      const response = await invoke('ask_question', {
        question,
        context: null,
        maxResults: 5,
      });

      const elapsed = performance.now() - startTime;

      // Track Q&A operation
      Telemetry.recordQA(
        question.length,
        response.answer.length,
        response.sources.length,
        elapsed
      );

      setAnswer(response.answer);
      setSources(response.sources);
    } catch (error) {
      Telemetry.recordError(
        error as Error,
        'qa_interface',
        'error'
      );
      console.error('Q&A failed:', error);
    } finally {
      setIsAsking(false);
    }
  };

  return (
    <div className="qa-interface">
      <textarea
        value={question}
        onChange={(e) => setQuestion(e.target.value)}
        placeholder="Ask a question..."
      />
      <button onClick={handleAskQuestion} disabled={isAsking}>
        {isAsking ? 'Thinking...' : 'Ask'}
      </button>

      {answer && (
        <div className="answer">
          <h3>Answer:</h3>
          <p>{answer}</p>

          <h4>Sources:</h4>
          <ul>
            {sources.map((source, idx) => (
              <li key={idx}>
                {source.document_title} (score: {source.relevance_score})
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Example 5: Using useTelemetry Hook
// ============================================================================

export function SearchWithHook() {
  const telemetry = useTelemetry();
  const [query, setQuery] = useState('');

  const handleSearch = async () => {
    const endMeasurement = telemetry.startMeasurement('search_operation');

    try {
      const results = await invoke('search_documents', {
        options: { query, limit: 10 },
      });

      telemetry.recordSearch(query, results.results.length, 0);
    } catch (error) {
      telemetry.recordError(error as Error, 'search_with_hook');
    } finally {
      endMeasurement();
    }
  };

  return (
    <div>
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <button onClick={handleSearch}>Search</button>
    </div>
  );
}

// ============================================================================
// Example 6: Page View Tracking with HOC
// ============================================================================

function SettingsPageComponent() {
  const handleSaveSetting = (key: string, value: any) => {
    Telemetry.recordAction('save_setting', 'settings_page', {
      setting_key: key,
    });

    // Save setting...
  };

  return (
    <div className="settings-page">
      <h1>Settings</h1>
      {/* Settings UI */}
    </div>
  );
}

// Automatically tracks page views
export const SettingsPage = withTelemetry(SettingsPageComponent, 'settings');

// ============================================================================
// Example 7: Error Boundary with Telemetry
// ============================================================================

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class TelemetryErrorBoundary extends React.Component<
  { children: React.ReactNode },
  ErrorBoundaryState
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    // Track error with telemetry
    Telemetry.recordError(error, 'error_boundary', 'fatal');

    console.error('Error caught by boundary:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="error-boundary">
          <h1>Something went wrong</h1>
          <p>{this.state.error?.message}</p>
        </div>
      );
    }

    return this.props.children;
  }
}

// ============================================================================
// Example 8: Performance Monitoring
// ============================================================================

export function PerformanceMonitoredComponent() {
  useEffect(() => {
    const endMeasurement = Telemetry.startMeasurement('component_mount');

    // Component initialization logic...

    return () => {
      endMeasurement();
    };
  }, []);

  const handleExpensiveOperation = async () => {
    const endMeasurement = Telemetry.startMeasurement('expensive_operation');

    try {
      // Perform expensive operation...
      await someHeavyComputation();
    } finally {
      endMeasurement();
    }
  };

  return <div>...</div>;
}

// ============================================================================
// Example 9: Cache Hit/Miss Tracking
// ============================================================================

export function CachedDataComponent() {
  const [data, setData] = useState<any>(null);

  const loadData = async (key: string) => {
    // Check cache first
    const cached = getCachedData(key);

    if (cached) {
      Telemetry.recordCacheEvent(true, key);
      setData(cached);
      return;
    }

    // Cache miss
    Telemetry.recordCacheEvent(false, key);

    // Fetch fresh data
    const freshData = await fetchData(key);
    setCachedData(key, freshData);
    setData(freshData);
  };

  return <div>...</div>;
}

// ============================================================================
// Example 10: User Action Tracking
// ============================================================================

export function ButtonWithTelemetry() {
  const handleClick = () => {
    Telemetry.recordAction(
      'button_click',
      'action_button',
      {
        button_id: 'primary_action',
        location: 'header',
      }
    );

    // Perform action...
  };

  return (
    <button onClick={handleClick}>
      Primary Action
    </button>
  );
}

// Utility functions (mock implementations)
function getCachedData(key: string): any {
  return sessionStorage.getItem(key);
}

function setCachedData(key: string, data: any): void {
  sessionStorage.setItem(key, JSON.stringify(data));
}

async function fetchData(key: string): Promise<any> {
  // Fetch data from backend
  return {};
}

async function someHeavyComputation(): Promise<void> {
  // Heavy computation
}
