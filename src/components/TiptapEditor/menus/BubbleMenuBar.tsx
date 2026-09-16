import { useCallback, useEffect, useState } from 'react';
import type { ReactNode } from 'react';

import { BubbleMenu } from '@tiptap/react/menus';
import { Bold, Highlighter, Italic, Link as LinkIcon } from 'lucide-react';
import { useNavigate } from 'react-router';

import { clipboardUrl } from '../../QuickCapture/clipboard';

import type { Editor } from '@tiptap/react';

/**
 * A verb a surface adds to the selection toolbar (the Journal's "Save
 * highlight", for instance). Receives the selected plain text.
 */
export interface SelectionAction {
  id: string;
  label: string;
  onSelect: (text: string) => void;
}

interface BubbleMenuBarProps {
  editor: Editor;
  selectionActions?: SelectionAction[];
}

interface MenuButtonProps {
  label: string;
  isActive: boolean;
  onClick: () => void;
  children: ReactNode;
}

function MenuButton({ label, isActive, onClick, children }: MenuButtonProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={isActive}
      onMouseDown={(event) => {
        event.preventDefault();
        onClick();
      }}
      className={`flex h-6 w-6 items-center justify-center rounded-sm transition-colors duration-fast ${
        isActive ? 'text-text-primary' : 'text-text-tertiary hover:text-text-primary'
      }`}
    >
      {children}
    </button>
  );
}

/**
 * The selection toolbar: the four marks worth reaching for, plus the one verb
 * that leaves the editor. Formatting first, then a rule, then the verb — so the
 * destructive-to-flow action is never the thing under the cursor.
 */
export function BubbleMenuBar({ editor, selectionActions = [] }: BubbleMenuBarProps) {
  const navigate = useNavigate();
  const [linkMode, setLinkMode] = useState(false);
  const [linkDraft, setLinkDraft] = useState('');

  // The bubble is one long-lived element that follows the selection, so link
  // mode has to be closed when the selection moves — otherwise selecting a new
  // phrase reopens the toolbar on the abandoned URL input.
  useEffect(() => {
    const reset = () => {
      setLinkMode(false);
      setLinkDraft('');
    };
    // `selectionUpdate` only — not `blur`, which fires the moment the URL input
    // takes focus and would close the row the user just opened.
    editor.on('selectionUpdate', reset);
    return () => {
      editor.off('selectionUpdate', reset);
    };
  }, [editor]);

  const handleLink = useCallback(() => {
    if (editor.isActive('link')) {
      editor.chain().focus().unsetLink().run();
      return;
    }
    setLinkDraft('');
    setLinkMode(true);
  }, [editor]);

  const commitLink = useCallback(() => {
    const url = clipboardUrl(linkDraft.trim());
    if (url) {
      editor.chain().focus().extendMarkRange('link').setLink({ href: url.href }).run();
    }
    setLinkMode(false);
    setLinkDraft('');
  }, [editor, linkDraft]);

  const selectedText = useCallback(() => {
    const { from, to } = editor.state.selection;
    return editor.state.doc.textBetween(from, to, ' ').trim();
  }, [editor]);

  const handleAsk = useCallback(() => {
    const quote = selectedText().slice(0, 2000);
    if (!quote) return;
    navigate(`/chat?${new URLSearchParams({ new: '1', quote }).toString()}`);
  }, [navigate, selectedText]);

  return (
    <BubbleMenu
      editor={editor}
      options={{ placement: 'top', offset: 8 }}
      shouldShow={({ editor: instance, from, to }) =>
        instance.isEditable && from !== to && !instance.isActive('codeBlock') && !instance.isActive('image')
      }
      className="tiptap-bubble"
    >
      {linkMode ? (
        <input
          value={linkDraft}
          onChange={(event) => setLinkDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              commitLink();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              setLinkMode(false);
            }
          }}
          autoFocus
          placeholder="Paste a link"
          aria-label="Paste a link"
          className="tiptap-bubble-input"
        />
      ) : (
        <>
          <MenuButton
            label="Bold"
            isActive={editor.isActive('bold')}
            onClick={() => editor.chain().focus().toggleBold().run()}
          >
            <Bold className="h-3.5 w-3.5" strokeWidth={2} />
          </MenuButton>
          <MenuButton
            label="Italic"
            isActive={editor.isActive('italic')}
            onClick={() => editor.chain().focus().toggleItalic().run()}
          >
            <Italic className="h-3.5 w-3.5" strokeWidth={2} />
          </MenuButton>
          <MenuButton
            label="Highlight"
            isActive={editor.isActive('highlight')}
            onClick={() => editor.chain().focus().toggleHighlight().run()}
          >
            <Highlighter className="h-3.5 w-3.5" strokeWidth={2} />
          </MenuButton>
          <MenuButton
            label={editor.isActive('link') ? 'Remove link' : 'Link'}
            isActive={editor.isActive('link')}
            onClick={handleLink}
          >
            <LinkIcon className="h-3.5 w-3.5" strokeWidth={2} />
          </MenuButton>
          <span className="tiptap-bubble-sep" aria-hidden />
          {selectionActions.map((action) => (
            <button
              key={action.id}
              type="button"
              className="tiptap-bubble-verb"
              onMouseDown={(event) => {
                event.preventDefault();
                const text = selectedText();
                if (text) action.onSelect(text);
              }}
            >
              {action.label}
            </button>
          ))}
          <button
            type="button"
            className="tiptap-bubble-verb"
            onMouseDown={(event) => {
              event.preventDefault();
              handleAsk();
            }}
          >
            Ask about this
          </button>
        </>
      )}
    </BubbleMenu>
  );
}
