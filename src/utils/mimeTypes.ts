export const MIME_BY_EXTENSION: Record<string, string> = {
  // Documents
  pdf: 'application/pdf',
  md: 'text/markdown',
  markdown: 'text/markdown',
  txt: 'text/plain',
  html: 'text/html',
  htm: 'text/html',
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',

  // Spreadsheets
  xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  xls: 'application/vnd.ms-excel',
  csv: 'text/csv',
  tsv: 'text/tab-separated-values',

  // Presentations
  pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  ppt: 'application/vnd.ms-powerpoint',

  // Archive / text
  rtf: 'application/rtf',
  odt: 'application/vnd.oasis.opendocument.text',
  json: 'application/json',
  xml: 'application/xml',

  // Images
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  svg: 'image/svg+xml',
  bmp: 'image/bmp',
  ico: 'image/x-icon',

  // Audio
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  wav: 'audio/wav',
  flac: 'audio/flac',
  ogg: 'audio/ogg',
  aac: 'audio/aac',
  wma: 'audio/x-ms-wma',

  // Source code
  rs: 'text/x-rust',
  py: 'text/x-python',
  js: 'text/javascript',
  mjs: 'text/javascript',
  cjs: 'text/javascript',
  jsx: 'text/javascript-jsx',
  ts: 'text/typescript',
  tsx: 'text/typescript-jsx',
  go: 'text/x-go',
  java: 'text/x-java',
  c: 'text/x-c',
  h: 'text/x-c',
  cpp: 'text/x-c++',
  cc: 'text/x-c++',
  cxx: 'text/x-c++',
  hpp: 'text/x-c++',
  cs: 'text/x-csharp',
  rb: 'text/x-ruby',
  php: 'text/x-php',
  swift: 'text/x-swift',
  kt: 'text/x-kotlin',
  scala: 'text/x-scala',
  sh: 'text/x-shellscript',
  bash: 'text/x-shellscript',
  zsh: 'text/x-shellscript',
  yaml: 'text/x-yaml',
  yml: 'text/x-yaml',
  toml: 'text/x-toml',
  sql: 'text/x-sql',
  css: 'text/css',
  scss: 'text/x-scss',
  sass: 'text/x-scss',
  less: 'text/x-scss',
  lua: 'text/x-lua',
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
