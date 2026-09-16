const SUPPORTED_EXTENSIONS = new Set<string>([
  // Documents
  'txt',
  'md',
  'markdown',
  'pdf',
  'docx',
  'rtf',
  'odt',
  'xlsx',
  'pptx',
  // Code - Rust
  'rs',
  'toml',
  // Code - Web
  'js',
  'mjs',
  'cjs',
  'ts',
  'tsx',
  'jsx',
  'html',
  'htm',
  'css',
  'scss',
  'sass',
  'less',
  // Code - Python
  'py',
  'pyw',
  'pyi',
  // Code - Systems
  'c',
  'h',
  'cpp',
  'cxx',
  'cc',
  'hpp',
  'hxx',
  'hh',
  'go',
  // Code - JVM
  'java',
  'kt',
  'kts',
  'scala',
  'clj',
  'cljs',
  'cljc',
  // Code - Other
  'rb',
  'rake',
  'php',
  'swift',
  'r',
  'm',
  'ex',
  'exs',
  'erl',
  'hrl',
  // Shell scripts
  'sh',
  'bash',
  'zsh',
  'fish',
  'ps1',
  'psm1',
  'bat',
  'cmd',
  // Config/Data formats
  'json',
  'xml',
  'yaml',
  'yml',
  'ini',
  'conf',
  'config',
  // Database
  'sql',
  'graphql',
  'gql',
  // Data files
  'csv',
  'tsv',
]);

const BLOCKED_EXTENSIONS = new Set<string>([
  'exe',
  'dll',
  'dylib',
  'so',
  'bin',
  'msi',
  'app',
  'apk',
  'deb',
  'rpm',
  'pkg',
  'dmg',
  'iso',
  // Archives
  'zip',
  'rar',
  '7z',
  'tar',
  'gz',
  'bz2',
  'xz',
  'tgz',
]);

export type ValidationResult =
  | { ok: true; extension: string }
  | { ok: false; reason: string; extension?: string };

export function getExtensionFromPath(path: string): string | null {
  const fileName = path.split(/[\\/]/).pop() ?? path;
  const dotIndex = fileName.lastIndexOf('.');
  if (dotIndex <= 0 || dotIndex === fileName.length - 1) {
    return null;
  }
  return fileName.slice(dotIndex + 1).toLowerCase();
}

export function validateIndexablePath(path: string): ValidationResult {
  const extension = getExtensionFromPath(path);
  if (!extension) {
    return { ok: false, reason: 'Missing file extension' };
  }
  if (BLOCKED_EXTENSIONS.has(extension)) {
    return {
      ok: false,
      reason: 'Executables and archives are not supported',
      extension,
    };
  }
  if (!SUPPORTED_EXTENSIONS.has(extension)) {
    return { ok: false, reason: 'Unsupported file type', extension };
  }
  return { ok: true, extension };
}

export function filterIndexablePaths(paths: string[]): {
  accepted: string[];
  rejected: Array<{ path: string; reason: string; extension?: string }>;
} {
  const accepted: string[] = [];
  const rejected: Array<{ path: string; reason: string; extension?: string }> = [];

  for (const path of paths) {
    const validation = validateIndexablePath(path);
    if (validation.ok) {
      accepted.push(path);
    } else {
      rejected.push({
        path,
        reason: validation.reason,
        extension: validation.extension,
      });
    }
  }

  return { accepted, rejected };
}

export const SUPPORTED_EXTENSIONS_LIST = Array.from(SUPPORTED_EXTENSIONS.values());
