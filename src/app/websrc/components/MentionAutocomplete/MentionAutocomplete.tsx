import { useState, useEffect, useRef, useCallback } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { User, Lightbulb, FileText } from 'lucide-react';

interface Mention {
  id: string;
  name: string;
  type: 'person' | 'concept' | 'wikilink';
  metadata?: string;
  createdAt: string;
}

interface MentionAutocompleteProps {
  query: string;
  position: { top: number; left: number };
  onSelect: (mention: Mention) => void;
  onClose: () => void;
  type: 'mention' | 'wikilink';
}

export function MentionAutocomplete({
  query,
  position,
  onSelect,
  onClose,
  type,
}: MentionAutocompleteProps) {
  const [mentions, setMentions] = useState<Mention[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const searchMentions = useCallback(async (searchQuery: string) => {
    if (!searchQuery) {
      setMentions([]);
      return;
    }

    setLoading(true);
    try {
      const result = await invoke<{ mentions: Mention[] }>('search_mentions', {
        query: searchQuery,
        limit: 10,
      });

      const filteredMentions =
        type === 'mention'
          ? result.mentions.filter((m) => m.type === 'person' || m.type === 'concept')
          : result.mentions.filter((m) => m.type === 'wikilink');

      setMentions(filteredMentions);
      setSelectedIndex(0);
    } catch (err) {
      console.error('Failed to search mentions:', err);
      setMentions([]);
    } finally {
      setLoading(false);
    }
  }, [type]);

  // PERFORMANCE FIX: Debounce search to reduce database load
  useEffect(() => {
    const timeoutId = setTimeout(() => {
      searchMentions(query);
    }, 300); // 300ms debounce

    return () => clearTimeout(timeoutId);
  }, [query, searchMentions]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (mentions.length === 0 && e.key !== 'Escape') return;

      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((prev) => (prev + 1) % mentions.length);
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((prev) => (prev - 1 + mentions.length) % mentions.length);
          break;
        case 'Enter':
          e.preventDefault();
          if (mentions[selectedIndex]) {
            onSelect(mentions[selectedIndex]);
          }
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [mentions, selectedIndex, onSelect, onClose]);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && e.target instanceof Node && !containerRef.current.contains(e.target)) {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  const getIcon = (mentionType: string) => {
    switch (mentionType) {
      case 'person':
        return <User className="w-4 h-4 text-[var(--accent-primary)]" />;
      case 'concept':
        return <Lightbulb className="w-4 h-4 text-[var(--warning)]" />;
      case 'wikilink':
        return <FileText className="w-4 h-4 text-[var(--success)]" />;
      default:
        return <FileText className="w-4 h-4 text-[var(--text-tertiary)]" />;
    }
  };

  if (loading && mentions.length === 0) {
    return (
      <div
        ref={containerRef}
        className="absolute z-50 min-w-[250px] bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg shadow-lg p-3"
        style={{ top: position.top, left: position.left }}
      >
        <div className="flex items-center justify-center">
          <div className="animate-spin h-5 w-5 border-2 border-[var(--accent-primary)] border-t-transparent rounded-full" />
          <span className="ml-2 text-sm text-[var(--text-secondary)]">Searching...</span>
        </div>
      </div>
    );
  }

  if (mentions.length === 0 && !loading) {
    return (
      <div
        ref={containerRef}
        className="absolute z-50 min-w-[250px] bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg shadow-lg p-3"
        style={{ top: position.top, left: position.left }}
      >
        <div className="text-sm text-[var(--text-secondary)] text-center">
          {query ? `No matches for "${query}"` : type === 'mention' ? 'Type to search people or concepts' : 'Type to search notes'}
        </div>
        {query && (
          <div className="mt-2 text-xs text-[var(--text-tertiary)] text-center">
            Press Enter to create new
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="absolute z-50 min-w-[300px] max-w-[400px] bg-[var(--surface-elevated)] border border-[var(--border-color)] rounded-lg shadow-lg overflow-hidden"
      style={{ top: position.top, left: position.left }}
    >
      <div className="max-h-[300px] overflow-y-auto">
        {mentions.map((mention, index) => (
          <button
            key={mention.id}
            onClick={() => onSelect(mention)}
            className={`w-full px-4 py-2 flex items-center gap-3 text-left transition-colors
              ${
                index === selectedIndex
                  ? 'bg-[var(--accent-light)]/20 border-l-2 border-[var(--accent-primary)]'
                  : 'hover:bg-[var(--bg-secondary)]'
              }`}
          >
            {getIcon(mention.type)}
            <div className="flex-1 min-w-0">
              <div className="font-medium text-[var(--text-primary)] truncate">
                {mention.name}
              </div>
              <div className="text-xs text-[var(--text-secondary)] capitalize">
                {mention.type}
              </div>
            </div>
          </button>
        ))}
      </div>

      <div className="px-3 py-2 bg-[var(--bg-secondary)] border-t text-xs text-[var(--text-secondary)]">
        <div className="flex items-center justify-between">
          <span>↑↓ Navigate</span>
          <span>↵ Select</span>
          <span>Esc Close</span>
        </div>
      </div>
    </div>
  );
}
