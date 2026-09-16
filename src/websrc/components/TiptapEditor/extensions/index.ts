import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import Highlight from '@tiptap/extension-highlight';
import Image from '@tiptap/extension-image';
import Link from '@tiptap/extension-link';
import { Placeholder } from '@tiptap/extension-placeholder';
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table';
import { TaskItem } from '@tiptap/extension-task-item';
import { TaskList } from '@tiptap/extension-task-list';
import StarterKit from '@tiptap/starter-kit';
import { common, createLowlight } from 'lowlight';

import { SlashMenu, type SlashMenuOptions } from './slashMenu';
import { Wikilink } from './wikilink';

import type { AnyExtension } from '@tiptap/core';

const lowlight = createLowlight(common);

export interface ExtensionConfig {
  placeholder?: string;
  editable?: boolean;
  onWikilinkClick?: (title: string) => void;
  /** Enables the `/` menu. Omitted by read-only viewers. */
  slashMenu?: SlashMenuOptions;
}

export function createExtensions(config: ExtensionConfig = {}): AnyExtension[] {
  return [
    StarterKit.configure({
      codeBlock: false, // replaced by CodeBlockLowlight
      link: false, // registered below with custom attributes
    }),
    CodeBlockLowlight.configure({
      lowlight,
      defaultLanguage: 'plaintext',
    }),
    Table.configure({ resizable: false }),
    TableRow,
    TableHeader,
    TableCell,
    TaskList,
    TaskItem.configure({ nested: true }),
    Link.configure({
      openOnClick: true,
      autolink: true,
      HTMLAttributes: {
        target: '_blank',
        rel: 'noopener noreferrer',
        class: 'text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))] underline transition-colors duration-fast',
      },
    }),
    Image.configure({
      inline: true,
      allowBase64: true,
    }),
    Highlight.configure({
      multicolor: true,
    }),
    Wikilink.configure({
      onWikilinkClick: config.onWikilinkClick,
    }),
    ...(config.slashMenu ? [SlashMenu.configure(config.slashMenu)] : []),
    ...(config.placeholder
      ? [Placeholder.configure({ placeholder: config.placeholder })]
      : []),
  ];
}
