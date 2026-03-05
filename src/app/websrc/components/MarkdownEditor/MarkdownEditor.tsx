import { useState, useRef, useCallback, useEffect } from 'react';

import { invoke } from '@tauri-apps/api/core';

import { MentionAutocomplete } from '../MentionAutocomplete';
import { TiptapEditor } from '../TiptapEditor';

interface Mention {
  id: string;
  name: string;
  type: 'person' | 'concept' | 'wikilink';
  metadata?: string;
  createdAt: string;
}

interface MarkdownEditorProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
  documentId?: string;
  onWikilinkClick?: (title: string) => void;
}

interface AutocompleteState {
  show: boolean;
  query: string;
  type: 'mention' | 'wikilink';
  position: { top: number; left: number };
  triggerOffset: number;
}

/**
 * Markdown editor with wikilink and mention autocomplete.
 * Now powered by Tiptap instead of a plain textarea.
 */
export function MarkdownEditor({
  value,
  onChange,
  placeholder = 'Start writing... Use [[wikilinks]] to link notes or @[mentions] for people.',
  className = '',
  documentId: _documentId,
  onWikilinkClick,
}: MarkdownEditorProps) {
  const editorWrapperRef = useRef<HTMLDivElement>(null);
  const lastMarkdownRef = useRef(value);
  const [autocomplete, setAutocomplete] = useState<AutocompleteState>({
    show: false,
    query: '',
    type: 'wikilink',
    position: { top: 0, left: 0 },
    triggerOffset: 0,
  });

  const detectAutocomplete = useCallback((md: string) => {
    // Detect [[wikilink or @[mention patterns at end of text
    const wikilinkMatch = md.match(/\[\[([^\]]*?)$/);
    if (wikilinkMatch) {
      return { type: 'wikilink' as const, query: wikilinkMatch[1], offset: md.length - wikilinkMatch[0].length };
    }
    const mentionMatch = md.match(/@\[([^\]]*?)$/);
    if (mentionMatch) {
      return { type: 'mention' as const, query: mentionMatch[1], offset: md.length - mentionMatch[0].length };
    }
    return null;
  }, []);

  const handleChange = useCallback(
    (md: string) => {
      lastMarkdownRef.current = md;
      onChange(md);

      const trigger = detectAutocomplete(md);
      if (trigger) {
        // Position autocomplete near the editor bottom-left as a reasonable default
        const wrapper = editorWrapperRef.current;
        const rect = wrapper?.getBoundingClientRect();
        setAutocomplete({
          show: true,
          query: trigger.query,
          type: trigger.type,
          position: {
            top: (rect?.bottom ?? 200) + 4,
            left: rect?.left ?? 16,
          },
          triggerOffset: trigger.offset,
        });
      } else {
        setAutocomplete((prev) => (prev.show ? { ...prev, show: false } : prev));
      }
    },
    [onChange, detectAutocomplete],
  );

  const handleMentionSelect = useCallback(
    async (mention: Mention) => {
      const md = lastMarkdownRef.current;
      const prefix = autocomplete.type === 'wikilink' ? '[[' : '@[';
      const suffix = autocomplete.type === 'wikilink' ? ']]' : ']';

      const before = md.substring(0, autocomplete.triggerOffset);
      const newMd = `${before}${prefix}${mention.name}${suffix} `;

      onChange(newMd);
      lastMarkdownRef.current = newMd;
      setAutocomplete((prev) => ({ ...prev, show: false }));

      if (autocomplete.type === 'wikilink' && mention.type !== 'wikilink') {
        try {
          await invoke('create_mention', {
            name: mention.name,
            mentionType: 'wikilink',
            metadata: null,
          });
        } catch (err) {
          console.error('Failed to create wikilink mention:', err);
        }
      }
    },
    [autocomplete, onChange],
  );

  const handleCloseAutocomplete = useCallback(() => {
    setAutocomplete((prev) => ({ ...prev, show: false }));
  }, []);

  // Close autocomplete on Escape key
  useEffect(() => {
    if (!autocomplete.show) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setAutocomplete((prev) => ({ ...prev, show: false }));
      }
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [autocomplete.show]);

  return (
    <div className="relative" ref={editorWrapperRef}>
      <div
        className={`w-full min-h-[400px] p-4 text-sm border rounded-lg
                   focus-within:outline-none focus-within:ring-2 ring-[var(--accent-primary)]
                   bg-[var(--surface-elevated)]
                   text-[var(--text-primary)]
                   border-[var(--border-color)]
                   ${className}`}
      >
        <TiptapEditor
          value={value}
          onChange={handleChange}
          placeholder={placeholder}
          onWikilinkClick={onWikilinkClick}
          autofocus
        />
      </div>

      {autocomplete.show && (
        <MentionAutocomplete
          query={autocomplete.query}
          position={autocomplete.position}
          type={autocomplete.type}
          onSelect={handleMentionSelect}
          onClose={handleCloseAutocomplete}
        />
      )}

      <div className="mt-2 text-xs text-[var(--text-secondary)] flex gap-4">
        <span>
          <kbd className="px-2 py-1 bg-[var(--bg-tertiary)] rounded border border-[var(--border-color)]">
            [[
          </kbd>{' '}
          for wikilinks
        </span>
        <span>
          <kbd className="px-2 py-1 bg-[var(--bg-tertiary)] rounded border border-[var(--border-color)]">
            @[
          </kbd>{' '}
          for mentions
        </span>
      </div>
    </div>
  );
}

MarkdownEditor.displayName = 'MarkdownEditor';
