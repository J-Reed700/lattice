/**
 * Extension-to-MIME lookup kept in sync with the backend `FILE_TYPES` registry
 * (`src/utils/__tests__/mimeTypes.test.ts` enforces zero drift without pulling
 * `bindings.ts` into the initial startup bundle).
 */
export const MIME_BY_EXTENSION: Record<string, string> = {
  // Documents
  txt: 'text/plain',
  md: 'text/markdown',
  markdown: 'text/markdown',
  pdf: 'application/pdf',
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  doc: 'application/msword',
  rtf: 'application/rtf',
  odt: 'application/vnd.oasis.opendocument.text',
  html: 'text/html',
  htm: 'text/html',

  // Spreadsheets & Presentations
  xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  xls: 'application/vnd.ms-excel',
  pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  ppt: 'application/vnd.ms-powerpoint',
  csv: 'text/csv',
  tsv: 'text/tab-separated-values',
  parquet: 'application/octet-stream',
  arrow: 'application/octet-stream',

  // Data & Config
  json: 'application/json',
  xml: 'application/xml',
  yaml: 'text/x-yaml',
  yml: 'text/x-yaml',
  toml: 'text/x-toml',
  ini: 'text/x-ini',
  conf: 'text/plain',
  config: 'text/plain',
  sql: 'text/x-sql',
  graphql: 'application/graphql',
  gql: 'application/graphql',

  // Code
  rs: 'text/x-rust',
  js: 'text/javascript',
  mjs: 'text/javascript',
  cjs: 'text/javascript',
  ts: 'text/typescript',
  tsx: 'text/typescript-jsx',
  jsx: 'text/javascript-jsx',
  css: 'text/css',
  scss: 'text/x-scss',
  sass: 'text/x-scss',
  less: 'text/x-less',
  py: 'text/x-python',
  pyw: 'text/x-python',
  pyi: 'text/x-python',
  c: 'text/x-c',
  h: 'text/x-c-header',
  cpp: 'text/x-c++',
  cxx: 'text/x-c++',
  cc: 'text/x-c++',
  hpp: 'text/x-c++-header',
  hxx: 'text/x-c++-header',
  hh: 'text/x-c++-header',
  go: 'text/x-go',
  java: 'text/x-java',
  kt: 'text/x-kotlin',
  kts: 'text/x-kotlin',
  scala: 'text/x-scala',
  clj: 'text/x-clojure',
  cljs: 'text/x-clojure',
  cljc: 'text/x-clojure',
  rb: 'text/x-ruby',
  rake: 'text/x-ruby',
  php: 'text/x-php',
  swift: 'text/x-swift',
  r: 'text/x-r',
  m: 'text/x-matlab',
  mm: 'text/x-objective-c',
  pl: 'text/x-perl',
  pm: 'text/x-perl',
  lua: 'text/x-lua',
  hs: 'text/x-haskell',
  lhs: 'text/x-haskell',
  cs: 'text/x-csharp',
  vb: 'text/x-vb',
  fs: 'text/x-fsharp',
  ex: 'text/x-elixir',
  exs: 'text/x-elixir',
  erl: 'text/x-erlang',
  hrl: 'text/x-erlang',
  sh: 'text/x-shellscript',
  bash: 'text/x-bash',
  zsh: 'text/x-zsh',
  fish: 'text/x-fish',
  ps1: 'text/x-powershell',
  psm1: 'text/x-powershell',
  bat: 'text/x-batch',
  cmd: 'text/x-batch',

  // Images
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  png: 'image/png',
  gif: 'image/gif',
  bmp: 'image/bmp',
  svg: 'image/svg+xml',
  webp: 'image/webp',
  ico: 'image/x-icon',
  tiff: 'image/tiff',
  tif: 'image/tiff',
  heic: 'image/heic',
  heif: 'image/heif',

  // Video
  mp4: 'video/mp4',
  mov: 'video/quicktime',
  avi: 'video/x-msvideo',
  mkv: 'video/x-matroska',
  webm: 'video/webm',
  flv: 'video/x-flv',
  wmv: 'video/x-ms-wmv',
  m4v: 'video/x-m4v',

  // Audio
  mp3: 'audio/mpeg',
  wav: 'audio/wav',
  ogg: 'audio/ogg',
  flac: 'audio/flac',
  aac: 'audio/aac',
  m4a: 'audio/mp4',
  wma: 'audio/x-ms-wma',

  // Archives & Binaries
  zip: 'application/zip',
  tar: 'application/x-tar',
  gz: 'application/gzip',
  tgz: 'application/gzip',
  bz2: 'application/x-bzip2',
  '7z': 'application/x-7z-compressed',
  rar: 'application/vnd.rar',
  xz: 'application/x-xz',
  exe: 'application/octet-stream',
  dll: 'application/octet-stream',
  dylib: 'application/octet-stream',
  so: 'application/octet-stream',
  bin: 'application/octet-stream',
  msi: 'application/octet-stream',
  app: 'application/octet-stream',
  apk: 'application/octet-stream',
  deb: 'application/octet-stream',
  rpm: 'application/octet-stream',
  pkg: 'application/octet-stream',
  dmg: 'application/octet-stream',
  iso: 'application/octet-stream',
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
  const base = filePath.split(/[\\/]/).pop() ?? '';
  const dot = base.lastIndexOf('.');
  if (dot < 0 || dot === base.length - 1) return '';
  const extension = base.slice(dot + 1).toLowerCase();
  return MIME_BY_EXTENSION[extension] ?? '';
}
