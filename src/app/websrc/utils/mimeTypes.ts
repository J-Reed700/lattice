const MIME_BY_EXTENSION: Record<string, string> = {
  pdf: 'application/pdf',
  md: 'text/markdown',
  markdown: 'text/markdown',
  txt: 'text/plain',
  html: 'text/html',
  htm: 'text/html',
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  wav: 'audio/wav',
};

/**
 * The mime type a file path implies, or `''` when nothing is known.
 *
 * The preview modal picks its viewer from the mime type: without one a PDF is
 * read as UTF-8 text and the reader errors instead of opening. Saved passages
 * and compare rows carry no mime type, so it is derived from the path they do
 * carry — in one place, so both surfaces open the same file the same way.
 */
export function mimeTypeForPath(filePath: string): string {
  const extension = filePath.split('.').pop()?.toLowerCase() ?? '';
  return MIME_BY_EXTENSION[extension] ?? '';
}
