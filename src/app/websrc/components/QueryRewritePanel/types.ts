export interface QueryVariant {
  query: string;
  reasoning: string;
}

export interface QueryRewritePanelProps {
  /** The original search query to rewrite */
  originalQuery: string;
  /** Callback when a variant is selected */
  onVariantSelect: (query: string) => void;
  /** Callback to close the panel */
  onClose: () => void;
  /** Whether the panel is visible */
  isVisible?: boolean;
  /** Auto-generate on mount */
  autoGenerate?: boolean;
}
