import { useState, useEffect, useCallback } from 'react';

import { invoke } from '@tauri-apps/api/core';

import { TauriEventNames, EventSchemas, listenValidated } from '../../../types/events';
import { toErrorMessage } from '../../../utils/errorConverter';
import { type QueryVariant } from '../types';

interface UseQueryRewriteOptions {
  originalQuery: string;
  autoGenerate?: boolean;
  isVisible?: boolean;
}

interface UseQueryRewriteResult {
  variants: QueryVariant[];
  isGenerating: boolean;
  error: string | null;
  streamingResponse: string;
  generateRewrites: () => Promise<void>;
}

export function useQueryRewrite({
  originalQuery,
  autoGenerate = true,
  isVisible = true,
}: UseQueryRewriteOptions): UseQueryRewriteResult {
  const [variants, setVariants] = useState<QueryVariant[]>([]);
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [streamingResponse, setStreamingResponse] = useState('');

  // Parse streaming response to extract variants
  const parseVariants = useCallback(
    (text: string): QueryVariant[] => {
      const variants: QueryVariant[] = [];

      // Try to parse numbered variants with reasoning
      // Expected format:
      // 1. "query text"
      // Reasoning: explanation text
      //
      // 2. "query text"
      // Reasoning: explanation text

      const variantMatches = text.matchAll(
        /(\d+)\.\s*["']?([^"'\n]+)["']?(?:\s*\n\s*Reasoning:\s*([^\n]+))?/gi
      );

      for (const match of variantMatches) {
        const query = match[2]?.trim();
        const reasoning = match[3]?.trim() || 'Alternative query formulation';

        if (query && variants.length < 3) {
          variants.push({ query, reasoning });
        }
      }

      // Fallback: if no structured format, try to extract any quoted strings
      if (variants.length === 0) {
        const quotedMatches = text.matchAll(/["']([^"']+)["']/g);
        let count = 0;
        for (const match of quotedMatches) {
          if (count >= 3) break;
          const query = match[1]?.trim();
          if (query && query.length > 3 && query !== originalQuery) {
            variants.push({
              query,
              reasoning: 'Alternative query formulation',
            });
            count++;
          }
        }
      }

      return variants;
    },
    [originalQuery]
  );

  // Generate query rewrites using Ollama via ask_question_stream
  const generateRewrites = useCallback(async () => {
    if (!originalQuery.trim()) {
      setError('No query provided');
      return;
    }

    setIsGenerating(true);
    setError(null);
    setStreamingResponse('');
    setVariants([]);

    try {
      const prompt = `Rewrite this search query in 3 different ways to improve search results. For each rewrite, provide the query and a brief reasoning.

Original query: "${originalQuery}"

Please format your response as:
1. "rewritten query 1"
Reasoning: why this improves results

2. "rewritten query 2"
Reasoning: why this improves results

3. "rewritten query 3"
Reasoning: why this improves results`;

      // Use ask_question_stream command
      await invoke('ask_question_stream', {
        request: {
          question: prompt,
          context_limit: 0, // We don't need document context for query rewriting
        }
      });
    } catch (err) {
      setError(toErrorMessage(err));
      setIsGenerating(false);
    }
  }, [originalQuery]);

  // Listen for streaming response
  useEffect(() => {
    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    const setupListener = async () => {
      unlistenFn = await listenValidated(
        TauriEventNames.LLM.StreamChunk,
        EventSchemas.LLM.StreamChunk,
        (event) => {
        if (cancelled) return;
        const chunk = event.payload;

        if (chunk.type === 'token') {
          setStreamingResponse((prev) => {
            const newText = prev + chunk.content;
            const parsedVariants = parseVariants(newText);
            if (parsedVariants.length > 0) {
              setVariants(parsedVariants);
            }
            return newText;
          });
        } else if (chunk.type === 'done') {
          setIsGenerating(false);
          setStreamingResponse((currentResponse) => {
            const finalVariants = parseVariants(currentResponse);
            setVariants((prev) =>
              finalVariants.length > prev.length ? finalVariants : prev
            );
            return currentResponse;
          });
        } else if (chunk.type === 'error' && chunk.message) {
          console.error('[QueryRewrite] Stream error:', chunk.message);
          setIsGenerating(false);
        }
      },
        (error) => {
          console.error('[useQueryRewrite] Validation error for llm-stream:', error.format());
        }
      );
    };

    setupListener();

    return () => {
      cancelled = true;
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [parseVariants]);

  // Auto-generate on mount if enabled
  useEffect(() => {
    if (autoGenerate && isVisible && originalQuery) {
      generateRewrites();
    }
  }, [autoGenerate, isVisible, originalQuery, generateRewrites]);

  return {
    variants,
    isGenerating,
    error,
    streamingResponse,
    generateRewrites,
  };
}
