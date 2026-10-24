import { useCallback, useRef, useState, type ClipboardEvent, type DragEvent, type RefObject } from 'react';

import { useChatFileDrop } from '@/features/chat/hooks/useChatFileDrop';
import { pollAttachmentImport } from '@/features/chat/model/attachmentImportPoll';
import VaultAPI from '@/lib/api';

/** Something dropped or pasted on the review screen, ready for the Add a source sheet. */
export type DroppedSource =
  | { kind: 'link'; url: string }
  | { kind: 'text'; text: string; origin: string | null }
  | { kind: 'document'; documentId: string; name: string };

const URL_PATTERN = /^https?:\/\/\S+$/i;

const isTextField = (target: EventTarget | null) =>
  target instanceof HTMLElement && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);

/** A link, or text longer than one line; anything else is not a source. */
export function sourceFromText(raw: string, origin: string | null = null): DroppedSource | null {
  const text = raw.trim();
  if (!text) return null;
  if (URL_PATTERN.test(text)) return { kind: 'link', url: text };
  if (!/\n/.test(text)) return null;
  return { kind: 'text', text, origin };
}

/** The address the clipboard or a drag says the text came from, when it says one. */
function originOf(data: DataTransfer): string | null {
  const uri = data.getData('text/uri-list').split(/\r?\n/).find((line) => URL_PATTERN.test(line.trim()));
  if (uri) return uri.trim();
  const html = data.getData('text/html');
  return /SourceURL:(https?:\/\/[^\s"<>]+?)(?=-->|[\s"<>]|$)/.exec(html)?.[1] ?? null;
}

const basename = (path: string) => path.replace(/\\/g, '/').split('/').pop() || path;

/**
 * Drop and paste anywhere on the review screen : an address becomes a
 * link, text longer than a line becomes pasted text, and a file goes through
 * the library import the chat uses for dropped files, which gives the
 * document id the attach takes. Paste into a text field stays a paste.
 */
export function useReviewDrop(rootRef: RefObject<HTMLElement | null>, onSource: (source: DroppedSource) => void) {
  const [over, setOver] = useState(false);
  const [importing, setImporting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const onSourceRef = useRef(onSource);
  onSourceRef.current = onSource;

  const importFiles = useCallback(async (paths: string[]) => {
    const path = paths[0];
    if (!path) return;
    setError(null);
    setImporting(basename(path));
    try {
      const started = await VaultAPI.startBatchFileImport([path]);
      if (!started.ok) throw new Error(started.error);
      const outcome = await pollAttachmentImport(started.data, { getStatus: VaultAPI.getBatchJobStatus, intervalMs: 1000, timeoutMs: 5 * 60 * 1000 });
      if (outcome.kind === 'unreachable') throw new Error(outcome.error);
      const documentId = outcome.documentIds[0];
      if (!documentId) throw new Error(`${basename(path)} could not be imported.`);
      onSourceRef.current({ kind: 'document', documentId, name: basename(path) });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The file could not be imported.');
    } finally {
      setImporting(null);
    }
  }, []);

  // Files arrive through the webview's own drag-drop event, with their paths.
  const files = useChatFileDrop(rootRef, (paths) => { void importFiles(paths); }, () => false);

  const onDragOver = (event: DragEvent<HTMLElement>) => {
    if (![...event.dataTransfer.types].some((type) => type === 'text/uri-list' || type === 'text/plain' || type === 'Files')) return;
    event.preventDefault();
    setOver(true);
  };
  const onDragLeave = (event: DragEvent<HTMLElement>) => {
    if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
    setOver(false);
  };
  const onDrop = (event: DragEvent<HTMLElement>) => {
    setOver(false);
    const uri = event.dataTransfer.getData('text/uri-list');
    const source = sourceFromText(uri.split(/\r?\n/).find((line) => !line.startsWith('#')) ?? '') ?? sourceFromText(event.dataTransfer.getData('text/plain'), originOf(event.dataTransfer));
    if (!source) return;
    event.preventDefault();
    onSourceRef.current(source);
  };
  const onPaste = (event: ClipboardEvent<HTMLElement>) => {
    if (isTextField(event.target)) return;
    const source = sourceFromText(event.clipboardData.getData('text/plain'), originOf(event.clipboardData));
    if (!source) return;
    event.preventDefault();
    onSourceRef.current(source);
  };

  return {
    dragging: over || files.isDragging,
    importing,
    error,
    clearError: () => setError(null),
    handlers: { onDragOver, onDragLeave, onDrop, onPaste },
  };
}
