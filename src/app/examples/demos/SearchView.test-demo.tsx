/**
 * SearchView Test/Demo Component
 *
 * This file demonstrates and tests all search flow states:
 * 1. Welcome state (before first search)
 * 2. Loading state (skeleton cards)
 * 3. Results state (with mock data)
 * 4. Empty state (no results)
 * 5. Error state (search failed)
 *
 * To use this demo:
 * 1. Import in App.tsx: import { SearchViewDemo } from './components/SearchView/SearchView.test-demo';
 * 2. Replace SearchView with SearchViewDemo
 * 3. Use the state buttons to test different states
 */

import { useState } from 'react';
import { SearchBar } from '../SearchBar/SearchBar.enhanced';
import { ResultsList } from '../ResultsList/ResultsList.enhanced';
import type { SearchResult } from '../../types';

type DemoState = 'welcome' | 'loading' | 'results' | 'empty' | 'error';

export function SearchViewDemo() {
  const [query, setQuery] = useState('');
  const [mode, setMode] = useState<'semantic' | 'keyword' | 'hybrid'>('hybrid');
  const [demoState, setDemoState] = useState<DemoState>('welcome');

  // Mock search results
  const mockResults: SearchResult[] = [
    {
      id: '1',
      content:
        'Machine learning is a method of data analysis that automates analytical model building. It is a branch of artificial intelligence based on the idea that systems can learn from data.',
      metadata: {
        fileName: 'machine-learning-intro.md',
        path: '/documents/ml/machine-learning-intro.md',
        fileType: 'md',
        modifiedAt: '2024-11-10T10:30:00Z',
      },
      score: 0.95,
      vectorScore: 0.92,
      bm25Score: 0.88,
      vectorRank: 0,
      bm25Rank: 1,
    },
    {
      id: '2',
      content:
        'Deep learning is part of a broader family of machine learning methods based on artificial neural networks with representation learning. Learning can be supervised, semi-supervised or unsupervised.',
      metadata: {
        fileName: 'deep-learning-basics.pdf',
        path: '/documents/ml/deep-learning-basics.pdf',
        fileType: 'pdf',
        modifiedAt: '2024-11-09T15:45:00Z',
      },
      score: 0.87,
      vectorScore: 0.89,
      bm25Score: 0.75,
      vectorRank: 1,
      bm25Rank: 2,
    },
    {
      id: '3',
      content:
        'Neural networks are computing systems inspired by the biological neural networks that constitute animal brains. A neural network is based on a collection of connected units or nodes called artificial neurons.',
      metadata: {
        fileName: 'neural-networks.txt',
        path: '/documents/ml/neural-networks.txt',
        fileType: 'txt',
        modifiedAt: '2024-11-08T09:00:00Z',
      },
      score: 0.82,
      vectorScore: 0.85,
      bm25Score: 0.70,
      vectorRank: 2,
      bm25Rank: 3,
    },
  ];

  const handleSearch = (searchQuery: string, searchMode: 'semantic' | 'keyword' | 'hybrid') => {
    console.log('Search executed:', { searchQuery, searchMode });
    // In demo mode, you control the state manually with buttons
  };

  const handleResultClick = (result: SearchResult) => {
    console.log('Result clicked:', result);
    alert(`Opening: ${result.metadata.fileName}`);
  };

  // Determine what to show based on demo state
  const showResults = demoState === 'results' ? mockResults : [];
  const showLoading = demoState === 'loading';
  const showError = demoState === 'error' ? 'Failed to execute search. Please try again.' : null;
  const hasSearched = demoState !== 'welcome';

  return (
    <div className="flex-1 flex flex-col h-full bg-[var(--bg-secondary)]">
      {/* Demo Controls */}
      <div className="bg-[var(--warning-light)]/20 border-b border-[var(--warning-light)] px-6 py-3">
        <div className="flex items-center gap-4 flex-wrap">
          <span className="text-sm font-medium text-[var(--warning)]">
            Demo Mode:
          </span>
          <button
            onClick={() => setDemoState('welcome')}
            className={`px-3 py-1 text-xs rounded ${
              demoState === 'welcome'
                ? 'bg-[var(--warning)] text-white'
                : 'bg-[var(--warning-light)] text-[var(--warning)]'
            }`}
          >
            Welcome State
          </button>
          <button
            onClick={() => setDemoState('loading')}
            className={`px-3 py-1 text-xs rounded ${
              demoState === 'loading'
                ? 'bg-[var(--warning)] text-white'
                : 'bg-[var(--warning-light)] text-[var(--warning)]'
            }`}
          >
            Loading State
          </button>
          <button
            onClick={() => setDemoState('results')}
            className={`px-3 py-1 text-xs rounded ${
              demoState === 'results'
                ? 'bg-[var(--warning)] text-white'
                : 'bg-[var(--warning-light)] text-[var(--warning)]'
            }`}
          >
            Results State
          </button>
          <button
            onClick={() => setDemoState('empty')}
            className={`px-3 py-1 text-xs rounded ${
              demoState === 'empty'
                ? 'bg-[var(--warning)] text-white'
                : 'bg-[var(--warning-light)] text-[var(--warning)]'
            }`}
          >
            Empty State
          </button>
          <button
            onClick={() => setDemoState('error')}
            className={`px-3 py-1 text-xs rounded ${
              demoState === 'error'
                ? 'bg-[var(--warning)] text-white'
                : 'bg-[var(--warning-light)] text-[var(--warning)]'
            }`}
          >
            Error State
          </button>
        </div>
      </div>

      {/* Search Header */}
      <div className="bg-[var(--surface-elevated)] border-b border-[var(--border-color)] px-6 py-4 shadow-sm">
        <SearchBar
          query={query}
          onQueryChange={setQuery}
          onSearch={handleSearch}
          loading={showLoading}
          mode={mode}
          onModeChange={setMode}
        />
      </div>

      {/* Results Area */}
      <div className="flex-1 overflow-auto px-6 py-6">
        {!hasSearched ? (
          <WelcomeMessage mode={mode} />
        ) : (
          <ResultsList
            results={showResults}
            isLoading={showLoading}
            error={showError}
            onResultClick={handleResultClick}
          />
        )}
      </div>
    </div>
  );
}

// Welcome message (same as in SearchView.enhanced)
interface WelcomeMessageProps {
  mode: 'semantic' | 'keyword' | 'hybrid';
}

function WelcomeMessage({ mode }: WelcomeMessageProps) {
  const modeNames = {
    semantic: 'Semantic Search',
    keyword: 'Keyword Search',
    hybrid: 'Hybrid Search',
  };

  const modeDescriptions = {
    semantic: 'Find documents by meaning using AI-powered embeddings',
    keyword: 'Fast exact keyword matching with BM25 ranking',
    hybrid: 'Combine semantic understanding with keyword precision',
  };

  return (
    <div className="flex flex-col items-center justify-center h-full text-center px-4">
      <div className="w-20 h-20 bg-gradient-to-br from-blue-500 to-blue-600 rounded-2xl flex items-center justify-center mb-6 shadow-lg">
        <svg
          className="w-10 h-10 text-white"
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

      <h2 className="text-2xl font-bold text-[var(--text-primary)] mb-2">
        Search Your Documents
      </h2>

      <p className="text-[var(--text-secondary)] max-w-md mb-2">
        Start typing to search through your indexed documents
      </p>

      <p className="text-sm text-[var(--accent-primary)] mb-8">
        Using {modeNames[mode]} • {modeDescriptions[mode]}
      </p>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 max-w-3xl">
        <FeatureCard
          icon={
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"
              />
            </svg>
          }
          title="Semantic Search"
          description="Find documents by meaning, not just keywords"
          active={mode === 'semantic'}
        />
        <FeatureCard
          icon={
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"
              />
            </svg>
          }
          title="Hybrid Mode"
          description="Combine semantic and keyword search for best results"
          active={mode === 'hybrid'}
        />
        <FeatureCard
          icon={
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13 10V3L4 14h7v7l9-11h-7z"
              />
            </svg>
          }
          title="Lightning Fast"
          description="Instant results from your local document index"
          active={mode === 'keyword'}
        />
      </div>
    </div>
  );
}

interface FeatureCardProps {
  icon: React.ReactNode;
  title: string;
  description: string;
  active?: boolean;
}

function FeatureCard({ icon, title, description, active = false }: FeatureCardProps) {
  return (
    <div
      className={`
        bg-[var(--surface-elevated)] border rounded-lg p-4 text-left transition-all
        ${
          active
            ? 'border-[var(--accent-primary)] shadow-md'
            : 'border-[var(--border-color)]'
        }
      `}
    >
      <div className={`mb-2 ${active ? 'text-[var(--accent-primary)]' : 'text-[var(--text-secondary)]'}`}>
        {icon}
      </div>
      <h3 className="font-semibold text-[var(--text-primary)] mb-1">{title}</h3>
      <p className="text-sm text-[var(--text-secondary)]">{description}</p>
    </div>
  );
}
