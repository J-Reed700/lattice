import {
  CheckSquare,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Link2,
  List,
  ListOrdered,
  Table as TableIcon,
} from 'lucide-react';

import type { Editor } from '@tiptap/react';
import type { LucideIcon } from 'lucide-react';

export interface SlashRange {
  from: number;
  to: number;
}

export interface SlashItem {
  id: string;
  label: string;
  keywords: string[];
  icon: LucideIcon;
  run: (_editor: Editor, _range: SlashRange) => void;
}

/** Every run deletes the `/query` trigger first, so the block starts clean. */
export const SLASH_ITEMS: SlashItem[] = [
  {
    id: 'heading1',
    label: 'Heading 1',
    keywords: ['h1', 'title', 'big'],
    icon: Heading1,
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).setNode('heading', { level: 1 }).run(),
  },
  {
    id: 'heading2',
    label: 'Heading 2',
    keywords: ['h2', 'subtitle'],
    icon: Heading2,
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).setNode('heading', { level: 2 }).run(),
  },
  {
    id: 'heading3',
    label: 'Heading 3',
    keywords: ['h3'],
    icon: Heading3,
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).setNode('heading', { level: 3 }).run(),
  },
  {
    id: 'bulletList',
    label: 'Bulleted list',
    keywords: ['bullet', 'ul', 'list', 'unordered'],
    icon: List,
    run: (editor, range) => editor.chain().focus().deleteRange(range).toggleBulletList().run(),
  },
  {
    id: 'orderedList',
    label: 'Numbered list',
    keywords: ['number', 'ol', 'ordered'],
    icon: ListOrdered,
    run: (editor, range) => editor.chain().focus().deleteRange(range).toggleOrderedList().run(),
  },
  {
    id: 'taskList',
    label: 'To-do list',
    keywords: ['task', 'todo', 'checkbox', 'check'],
    icon: CheckSquare,
    run: (editor, range) => editor.chain().focus().deleteRange(range).toggleTaskList().run(),
  },
  {
    id: 'table',
    label: 'Table',
    keywords: ['grid', 'rows', 'columns'],
    icon: TableIcon,
    run: (editor, range) =>
      editor
        .chain()
        .focus()
        .deleteRange(range)
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run(),
  },
  {
    id: 'codeBlock',
    label: 'Code block',
    keywords: ['code', 'snippet', 'pre'],
    icon: Code,
    run: (editor, range) => editor.chain().focus().deleteRange(range).toggleCodeBlock().run(),
  },
  {
    id: 'wikilink',
    label: 'Link to a note',
    keywords: ['wikilink', '[[', 'reference', 'note'],
    icon: Link2,
    run: (editor, range) =>
      editor.chain().focus().deleteRange(range).insertContent('[[]]').setTextSelection(range.from + 2).run(),
  },
];

/**
 * Prefix-then-substring match over label and keywords; stable, case-insensitive.
 *
 * Prefix hits come first so `list` puts "Bulleted list" before "To-do list"
 * only when the label actually starts that way — matching what the eye expects
 * from the first letters typed.
 */
export function filterSlashItems(items: SlashItem[], query: string): SlashItem[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [...items];

  const prefix: SlashItem[] = [];
  const substring: SlashItem[] = [];

  for (const item of items) {
    const haystacks = [item.label.toLowerCase(), ...item.keywords.map((k) => k.toLowerCase())];
    if (haystacks.some((h) => h.startsWith(needle))) {
      prefix.push(item);
    } else if (haystacks.some((h) => h.includes(needle))) {
      substring.push(item);
    }
  }

  return [...prefix, ...substring];
}
