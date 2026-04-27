import { ErrorState } from './components/ErrorState';
import { KeyboardHints } from './components/KeyboardHints';
import { LoadingState } from './components/LoadingState';
import { OriginalQueryCard } from './components/OriginalQueryCard';
import { QueryRewriteHeader } from './components/QueryRewriteHeader';
import { VariantsList } from './components/VariantsList';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';
import { useQueryRewrite } from './hooks/useQueryRewrite';
import { type QueryRewritePanelProps } from './types';

/**
 * QueryRewritePanel
 *
 * Purpose: Generate and display alternative query formulations to improve search results
 *
 * Features:
 * - Shows original query + 3 AI-generated rewritten variants
 * - Each variant includes reasoning/rationale for the rewrite
 * - Click any variant to execute search with that query
 * - Keyboard shortcuts: 1-3 to select variants, Escape to close
 * - Loading state with streaming response
 * - Error handling for Ollama unavailability
 * - Highlight differences between original and rewrites
 *
 * States: idle, generating, ready, error
 * Accessibility: WCAG AA, keyboard navigation, screen reader friendly
 */
export function QueryRewritePanel({
  originalQuery,
  onVariantSelect,
  onClose,
  isVisible = true,
  autoGenerate = true,
}: QueryRewritePanelProps) {
  // Use custom hooks for business logic
  const { variants, isGenerating, error, streamingResponse, generateRewrites } =
    useQueryRewrite({
      originalQuery,
      autoGenerate,
      isVisible,
    });

  useKeyboardShortcuts({
    isVisible,
    variants,
    originalQuery,
    onVariantSelect,
    onClose,
  });

  if (!isVisible) return null;

  return (
    <div className="w-full space-y-4 p-4 bg-[hsl(var(--surface))]/50 rounded-lg border border-[hsl(var(--border-subtle))]">
      <QueryRewriteHeader
        isGenerating={isGenerating}
        onRegenerate={generateRewrites}
        onClose={onClose}
      />

      <OriginalQueryCard query={originalQuery} onSelect={() => onVariantSelect(originalQuery)} />

      {isGenerating && variants.length === 0 && <LoadingState />}

      {error && <ErrorState error={error} />}

      <VariantsList
        variants={variants}
        originalQuery={originalQuery}
        onVariantSelect={onVariantSelect}
        isGenerating={isGenerating}
        streamingResponse={streamingResponse}
      />

      <KeyboardHints />
    </div>
  );
}
