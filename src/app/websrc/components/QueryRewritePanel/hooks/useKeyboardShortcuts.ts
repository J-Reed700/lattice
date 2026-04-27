import { useEffect } from 'react';

import { type QueryVariant } from '../types';

interface UseKeyboardShortcutsOptions {
  isVisible: boolean;
  variants: QueryVariant[];
  originalQuery: string;
  onVariantSelect: (query: string) => void;
  onClose: () => void;
}

export function useKeyboardShortcuts({
  isVisible,
  variants,
  originalQuery,
  onVariantSelect,
  onClose,
}: UseKeyboardShortcutsOptions): void {
  useEffect(() => {
    if (!isVisible) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Number keys 1-3 to select variants
      if (['1', '2', '3'].includes(e.key) && variants.length >= parseInt(e.key)) {
        e.preventDefault();
        const variant = variants[parseInt(e.key) - 1];
        if (variant) {
          onVariantSelect(variant.query);
        }
      }

      // Escape to close
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }

      // 0 to select original query
      if (e.key === '0') {
        e.preventDefault();
        onVariantSelect(originalQuery);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isVisible, variants, originalQuery, onVariantSelect, onClose]);
}
