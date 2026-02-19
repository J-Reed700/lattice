import React, { useState, useEffect, useCallback } from 'react';

import { invoke } from '@tauri-apps/api/core';

import { useEffectiveTheme } from '../../hooks/useApplyTheme';
import { TauriEventNames, EventSchemas, listenValidated } from '../../types/events';
import { toErrorMessage } from '../../utils/errorConverter';

interface SourceReference {
  file_path: string;
  score: number;
  snippet: string;
}

interface QAResponse {
  answer: string;
  sources: SourceReference[];
}

export function QAPanel() {
  const [question, setQuestion] = useState('');
  const [response, setResponse] = useState<QAResponse | null>(null);
  const [streamingAnswer, setStreamingAnswer] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const theme = useEffectiveTheme();

  useEffect(() => {
    const unlisten = listenValidated(
      TauriEventNames.LLM.StreamChunk,
      EventSchemas.LLM.StreamChunk,
      (event) => {
      const chunk = event.payload;

      if (chunk.type === 'token') {
        setStreamingAnswer((prev) => prev + chunk.content);
      } else if (chunk.type === 'done') {
        setLoading(false);
        setStreamingAnswer((currentAnswer) => {
          setResponse({
            answer: currentAnswer,
            sources: [],
          });
          return currentAnswer;
        });
      } else if (chunk.type === 'sources' && chunk.sources) {
        // Handle sources if needed
        // Currently not implemented in the component
      } else if (chunk.type === 'error' && chunk.message) {
        console.error('[QAPanel] Stream error:', chunk.message);
        setLoading(false);
      }
    },
      (error) => {
        console.error('[QAPanel] Validation error for llm-stream:', error.format());
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []); // Fixed: removed streamingAnswer dependency to prevent listener recreation

  const handleAsk = useCallback(async () => {
    if (!question.trim()) return;

    setLoading(true);
    setError('');
    setResponse(null);
    setStreamingAnswer('');

    try {
      await invoke('ask_question_stream', {
        request: {
          question,
          context_limit: 5
        }
      });
    } catch (err) {
      setError(toErrorMessage(err));
      setLoading(false);
    }
  }, [question]);

  const handleKeyPress = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      handleAsk();
    }
  }, [handleAsk]);

  return (
    <div className={`qa-panel p-6 ${theme === 'dark' ? 'bg-[var(--surface-elevated)]' : 'bg-[var(--surface-elevated)]'} rounded-lg shadow-lg`}>
      <div className="bg-yellow-50 border-l-4 border-yellow-400 p-4 mb-4 rounded">
        <div className="flex items-center">
          <div className="flex-shrink-0">
            <svg className="h-5 w-5 text-yellow-400" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor">
              <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
            </svg>
          </div>
          <div className="ml-3">
            <p className="text-sm text-yellow-700">
              <strong>⚠️ Deprecated:</strong> QAPanel is being replaced. Please use the <a href="/chat" className="underline font-semibold">Chat tab</a> for improved conversation experience with document grounding and streaming.
            </p>
          </div>
        </div>
      </div>

      <div className="mb-4">
        <h2 className="text-2xl font-bold">
          Chat
        </h2>
        <p className="text-sm text-[var(--text-tertiary)] mt-1">
          Ask questions about your knowledge base
        </p>
        <div className={`text-sm ${theme === 'dark' ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-tertiary)]'} mt-2`}>
          Ctrl/Cmd + Enter to ask
        </div>
      </div>

      <div className="mb-4">
        <textarea
          className={`w-full p-3 border rounded-lg focus:ring-2 focus:ring-[var(--accent-primary)] focus:border-transparent ${
            theme === 'dark'
              ? 'bg-[var(--surface-elevated)] border-[var(--border-color)] text-white placeholder-[var(--text-tertiary)]'
              : 'bg-[var(--surface-elevated)] border-[var(--border-color)] text-[var(--text-primary)] placeholder-[var(--text-tertiary)]'
          }`}
          rows={3}
          placeholder="Ask anything about your documents..."
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyPress={handleKeyPress}
          disabled={loading}
        />
      </div>

      <button
        className={`px-6 py-2 rounded-lg font-medium transition-colors ${
          loading || !question.trim()
            ? theme === 'dark'
              ? 'bg-[var(--text-secondary)] text-[var(--text-tertiary)] cursor-not-allowed'
              : 'bg-[var(--bg-tertiary)] text-[var(--text-tertiary)] cursor-not-allowed'
            : theme === 'dark'
              ? 'bg-[var(--accent-primary)] text-white hover:bg-[var(--accent-primary)]'
              : 'bg-[var(--accent-primary)] text-white hover:bg-[var(--accent-primary)]'
        }`}
        onClick={handleAsk}
        disabled={loading || !question.trim()}
      >
        {loading ? (
          <span className="flex items-center">
            <svg className="animate-spin -ml-1 mr-3 h-5 w-5 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
            Thinking...
          </span>
        ) : (
          'Ask'
        )}
      </button>

      {(streamingAnswer || response) && (
        <div className={`mt-6 p-4 rounded-lg ${
          theme === 'dark' ? 'bg-[var(--surface-elevated)]' : 'bg-[var(--bg-secondary)]'
        }`}>
          <h3 className={`font-semibold mb-2 ${
            theme === 'dark' ? 'text-white' : 'text-[var(--text-primary)]'
          }`}>
            Answer:
          </h3>
          <p className={`whitespace-pre-wrap mb-4 ${
            theme === 'dark' ? 'text-[var(--text-secondary)]' : 'text-[var(--text-primary)]'
          }`}>
            {streamingAnswer || response?.answer}
            {loading && <span className="animate-pulse">▋</span>}
          </p>

          {response?.sources && response.sources.length > 0 && (
            <div className="mt-4">
              <h4 className={`font-semibold mb-2 text-sm ${
                theme === 'dark' ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-secondary)]'
              }`}>
                Sources ({response.sources.length}):
              </h4>
              <div className="space-y-2">
                {response.sources.map((source, idx) => (
                  <div
                    key={idx}
                    className={`p-3 rounded border ${
                      theme === 'dark'
                        ? 'bg-[var(--surface-elevated)] border-[var(--border-color)]'
                        : 'bg-[var(--surface-elevated)] border-[var(--border-color)]'
                    }`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <span className={`font-medium text-sm ${
                        theme === 'dark' ? 'text-[var(--accent-light)]' : 'text-[var(--accent-primary)]'
                      }`}>
                        {source.file_path}
                      </span>
                      <span className={`text-xs ${
                        theme === 'dark' ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-tertiary)]'
                      }`}>
                        Relevance: {(source.score * 100).toFixed(1)}%
                      </span>
                    </div>
                    <p className={`text-sm ${
                      theme === 'dark' ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-secondary)]'
                    }`}>
                      {source.snippet.substring(0, 150)}
                      {source.snippet.length > 150 && '...'}
                    </p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {error && (
        <div className={`mt-4 p-4 rounded-lg border ${
          theme === 'dark'
            ? 'bg-[var(--error)] border-[var(--error)]'
            : 'bg-[var(--error-light)] border-[var(--error)]'
        }`}>
          <p className={theme === 'dark' ? 'text-[var(--error-light)]' : 'text-[var(--error)]'}>
            {error}
          </p>
        </div>
      )}
    </div>
  );
}
