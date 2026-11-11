import { formatOfExtension, isIndexableFormat } from './fileTypeCatalogue';

export type FileType = 'markdown' | 'code' | 'text' | 'html' | 'docx' | 'pdf' | 'image' | 'audio' | 'unsupported';

export interface FileTypeInfo {
  type: FileType;
  language?: string;
  canPreview: boolean;
}

const MARKDOWN_EXTENSIONS = ['.md', '.markdown', '.mdown', '.mkd'];

const CODE_EXTENSIONS: Record<string, string> = {
  '.ts': 'typescript',
  '.tsx': 'typescript',
  '.js': 'javascript',
  '.jsx': 'javascript',
  '.py': 'python',
  '.rs': 'rust',
  '.go': 'go',
  '.java': 'java',
  '.c': 'c',
  '.cpp': 'cpp',
  '.h': 'c',
  '.hpp': 'cpp',
  '.cs': 'csharp',
  '.rb': 'ruby',
  '.php': 'php',
  '.swift': 'swift',
  '.kt': 'kotlin',
  '.scala': 'scala',
  '.sh': 'bash',
  '.bash': 'bash',
  '.zsh': 'bash',
  '.json': 'json',
  '.xml': 'xml',
  '.yaml': 'yaml',
  '.yml': 'yaml',
  '.toml': 'toml',
  '.sql': 'sql',
  '.css': 'css',
  '.scss': 'scss',
  '.sass': 'sass',
  '.vue': 'vue',
  '.svelte': 'svelte',
};

const TEXT_EXTENSIONS = [
  '.txt', '.log', '.cfg', '.conf', '.ini', '.env',
  '.gitignore', '.dockerignore', '.editorconfig',
];

const PDF_EXTENSIONS = ['.pdf'];

const IMAGE_EXTENSIONS = [
  '.jpg', '.jpeg', '.png', '.gif', '.bmp', '.svg', '.webp', '.ico',
];

export function detectFileType(filePath: string): FileTypeInfo {
  const lowerPath = filePath.toLowerCase();
  const extension = getExtension(lowerPath);

  if (MARKDOWN_EXTENSIONS.includes(extension)) {
    return { type: 'markdown', canPreview: true };
  }

  if (extension in CODE_EXTENSIONS) {
    return {
      type: 'code',
      language: CODE_EXTENSIONS[extension],
      canPreview: true,
    };
  }

  if (TEXT_EXTENSIONS.includes(extension)) {
    return { type: 'text', canPreview: true };
  }

  if (extension === '.html' || extension === '.htm') {
    return { type: 'html', canPreview: true };
  }

  if (extension === '.docx') {
    return { type: 'docx', canPreview: true };
  }

  if (PDF_EXTENSIONS.includes(extension)) {
    return { type: 'pdf', canPreview: true };
  }

  if (IMAGE_EXTENSIONS.includes(extension)) {
    return { type: 'image', canPreview: true };
  }

  const format = formatOfExtension(extension.slice(1));

  if (format?.category === 'audio') {
    return { type: 'audio', canPreview: true };
  }

  // Everything the indexer reads, the backend can also hand back as text.
  if (isIndexableFormat(format)) {
    return { type: 'text', canPreview: true };
  }

  return { type: 'unsupported', canPreview: false };
}

export function isSupportedFileType(filePath: string): boolean {
  const info = detectFileType(filePath);
  return info.canPreview;
}

function getExtension(filePath: string): string {
  const lastDot = filePath.lastIndexOf('.');
  if (lastDot === -1) return '';
  return filePath.substring(lastDot);
}
