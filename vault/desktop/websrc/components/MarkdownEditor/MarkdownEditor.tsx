import { useState, useRef, useCallback, useEffect } from 'react';

import { invoke } from '@tauri-apps/api/core';

import { MentionAutocomplete } from '../MentionAutocomplete';

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
  startPos: number;
}

export function MarkdownEditor({
  value,
  onChange,
  placeholder = 'Start writing... Use [[wikilinks]] to link notes or @[mentions] for people.',
  className = '',
  documentId: _documentId,
  onWikilinkClick,
}: MarkdownEditorProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [autocomplete, setAutocomplete] = useState<AutocompleteState>({
    show: false,
    query: '',
    type: 'wikilink',
    position: { top: 0, left: 0 },
    startPos: 0,
  });

  const getCaretCoordinates = useCallback(() => {
    if (!textareaRef.current) return { top: 0, left: 0 };

    const textarea = textareaRef.current;
    const style = window.getComputedStyle(textarea);
    const lineHeight = parseInt(style.lineHeight);

    const mirror = document.createElement('div');
    mirror.style.cssText = `
      position: absolute;
      visibility: hidden;
      white-space: pre-wrap;
      word-wrap: break-word;
      font-family: ${style.fontFamily};
      font-size: ${style.fontSize};
      line-height: ${style.lineHeight};
      padding: ${style.padding};
      border: ${style.border};
      width: ${textarea.offsetWidth}px;
    `;

    const textBeforeCaret = value.substring(0, textarea.selectionStart);
    mirror.textContent = textBeforeCaret;

    document.body.appendChild(mirror);
    const rect = textarea.getBoundingClientRect();
    const mirrorRect = mirror.getBoundingClientRect();
    document.body.removeChild(mirror);

    return {
      top: rect.top + mirrorRect.height + lineHeight,
      left: rect.left + (mirrorRect.width % textarea.offsetWidth),
    };
  }, [value]);

  const detectMentionTrigger = useCallback((text: string, cursorPos: number) => {
    const beforeCursor = text.substring(0, cursorPos);

    const wikilinkMatch = beforeCursor.match(/\[\[([^\]]*?)$/);
    if (wikilinkMatch) {
      return {
        type: 'wikilink' as const,
        query: wikilinkMatch[1],
        startPos: cursorPos - wikilinkMatch[1].length,
      };
    }

    const mentionMatch = beforeCursor.match(/@\[([^\]]*?)$/);
    if (mentionMatch) {
      return {
        type: 'mention' as const,
        query: mentionMatch[1],
        startPos: cursorPos - mentionMatch[1].length,
      };
    }

    return null;
  }, []);

  const handleInputChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      const newValue = e.target.value;
      const cursorPos = e.target.selectionStart;

      onChange(newValue);

      const trigger = detectMentionTrigger(newValue, cursorPos);

      if (trigger) {
        const coords = getCaretCoordinates();
        setAutocomplete({
          show: true,
          query: trigger.query,
          type: trigger.type,
          position: coords,
          startPos: trigger.startPos,
        });
      } else {
        setAutocomplete((prev) => ({ ...prev, show: false }));
      }
    },
    [onChange, detectMentionTrigger, getCaretCoordinates]
  );

  const handleMentionSelect = useCallback(
    async (mention: Mention) => {
      if (!textareaRef.current) return;

      const textarea = textareaRef.current;
      const cursorPos = textarea.selectionStart;

      const prefix = autocomplete.type === 'wikilink' ? '[[' : '@[';
      const suffix = autocomplete.type === 'wikilink' ? ']]' : ']';

      const beforeMention = value.substring(0, autocomplete.startPos - prefix.length);
      const afterCursor = value.substring(cursorPos);

      const newValue = `${beforeMention}${prefix}${mention.name}${suffix}${afterCursor}`;
      const newCursorPos = autocomplete.startPos - prefix.length + prefix.length + mention.name.length + suffix.length;

      onChange(newValue);

      setTimeout(() => {
        textarea.focus();
        textarea.setSelectionRange(newCursorPos, newCursorPos);
      }, 0);

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
    [autocomplete, value, onChange]
  );

  const handleCloseAutocomplete = useCallback(() => {
    setAutocomplete((prev) => ({ ...prev, show: false }));
  }, []);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    const handleClick = (e: MouseEvent) => {
      if (e.target instanceof HTMLElement && e.target.classList.contains('wikilink')) {
        e.preventDefault();
        const linkText = e.target.textContent?.replace(/^\[\[|\]\]$/g, '');
        if (linkText && onWikilinkClick) {
          onWikilinkClick(linkText);
        }
      }
    };

    textarea.addEventListener('click', handleClick);
    return () => textarea.removeEventListener('click', handleClick);
  }, [onWikilinkClick]);

  return (
    <div className="relative">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={handleInputChange}
        placeholder={placeholder}
        className={`w-full h-full min-h-[400px] p-4 font-mono text-sm border rounded-lg
                   focus:outline-none focus:ring-2 ring-[var(--accent-primary)]
                   bg-[var(--surface-elevated)]
                   text-[var(--text-primary)]
                   border-[var(--border-color)]
                   placeholder-[var(--text-tertiary)]
                   resize-none ${className}`}
      />

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
