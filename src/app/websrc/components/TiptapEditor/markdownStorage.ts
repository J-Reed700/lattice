import type { Editor } from '@tiptap/react';

interface MarkdownStorage {
  getMarkdown?: () => string;
}

export function getMarkdownFromEditor(editor: Editor): string {
  // tiptap-markdown stores its API under editor.storage.markdown
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const mdExt = (editor.storage as any).markdown as MarkdownStorage | undefined;
  return mdExt?.getMarkdown?.() ?? '';
}
