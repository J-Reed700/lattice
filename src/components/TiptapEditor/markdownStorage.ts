import type { Editor } from '@tiptap/react';

interface MarkdownStorage {
  getMarkdown?: () => string;
}

export function getMarkdownFromEditor(editor: Editor): string {
  // tiptap-markdown stores its API under editor.storage.markdown
  const mdExt = (editor.storage as { markdown?: MarkdownStorage }).markdown;
  return mdExt?.getMarkdown?.() ?? '';
}
