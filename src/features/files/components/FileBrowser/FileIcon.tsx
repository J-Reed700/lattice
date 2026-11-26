/**
 * File Icon Component
 *
 * Purpose: Display appropriate icon for file types
 */

import {
  File,
  FileText,
  FileCode,
  FileArchive,
  FileAudio,
  FileVideo,
  FileSpreadsheet,
  Image,
  Globe,
} from 'lucide-react';

import { type DocumentMetadata, getFileExtension } from '@/types/fileBrowser';

interface FileIconProps {
  file: DocumentMetadata;
  className?: string;
  size?: number;
}

/** The short label a reader recognises a format by. */
const FORMAT_LABELS: Record<string, string> = {
  pdf: 'PDF',
  md: 'MD',
  markdown: 'MD',
  txt: 'TXT',
  doc: 'DOC',
  docx: 'DOC',
  rtf: 'RTF',
  odt: 'ODT',
  epub: 'EPUB',
  html: 'HTML',
  htm: 'HTML',
  pptx: 'PPT',
  ppt: 'PPT',
  xlsx: 'XLS',
  xls: 'XLS',
  csv: 'CSV',
  json: 'JSON',
};

function getBadgeTone(isWeb: boolean | undefined, ext: string): string {
  if (isWeb || ext === 'html' || ext === 'htm') {
    return 'bg-sky-500/10 text-sky-700 ring-1 ring-inset ring-sky-500/15 dark:bg-sky-400/15 dark:text-sky-300 dark:ring-sky-400/20';
  }
  if (ext === 'md' || ext === 'markdown') {
    return 'bg-accent-muted text-accent ring-1 ring-inset ring-accent/15';
  }
  if (ext === 'pdf') {
    return 'bg-rose-500/10 text-rose-700 ring-1 ring-inset ring-rose-500/15 dark:bg-rose-400/15 dark:text-rose-300 dark:ring-rose-400/20';
  }
  if (['doc', 'docx', 'epub', 'rtf', 'odt', 'ppt', 'pptx'].includes(ext)) {
    return 'bg-amber-500/10 text-amber-700 ring-1 ring-inset ring-amber-500/15 dark:bg-amber-400/15 dark:text-amber-300 dark:ring-amber-400/20';
  }
  if (['csv', 'xls', 'xlsx'].includes(ext)) {
    return 'bg-emerald-500/10 text-emerald-700 ring-1 ring-inset ring-emerald-500/15 dark:bg-emerald-400/15 dark:text-emerald-300 dark:ring-emerald-400/20';
  }
  if (['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h', 'hpp', 'cs', 'rb', 'php', 'sh', 'zsh', 'bash', 'toml', 'css', 'scss', 'sql', 'json', 'xml', 'yml', 'yaml'].includes(ext)) {
    return 'bg-teal-500/10 text-teal-700 ring-1 ring-inset ring-teal-500/15 dark:bg-teal-400/15 dark:text-teal-300 dark:ring-teal-400/20';
  }
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'ico', 'bmp'].includes(ext)) {
    return 'bg-teal-500/10 text-teal-700 ring-1 ring-inset ring-teal-500/15 dark:bg-teal-400/15 dark:text-teal-300 dark:ring-teal-400/20';
  }
  return 'bg-slate-500/10 text-slate-700 ring-1 ring-inset ring-slate-500/15 dark:bg-slate-400/15 dark:text-slate-300 dark:ring-slate-400/20';
}

/**
 * Crisp color-coded format badge tile (`MD`, `PDF`, `Web`, `Code`, `DOC`, `CSV`).
 * Formats without a short familiar label fall back to a crisp Lucide glyph.
 */
export function FileIcon({ file, className = '', size = 16 }: FileIconProps) {
  const isWeb = file.web;
  const ext = (file.fileType || getFileExtension(file.fileName) || '').toLowerCase();
  const label = isWeb ? null : FORMAT_LABELS[ext];
  const tile = Math.round(size * 1.875);
  const Icon = getIconComponent(file);
  const tone = getBadgeTone(isWeb, ext);

  return (
    <span
      aria-hidden="true"
      style={{ width: tile, height: tile }}
      className={`inline-flex items-center justify-center rounded-lg ${tone} ${className}`}
    >
      {label ? (
        <span
          className="font-sans font-semibold leading-none tracking-[0.02em]"
          style={{ fontSize: label.length > 3 ? Math.max(7.5, size * 0.45) : Math.max(8.5, size * 0.54) }}
        >
          {label}
        </span>
      ) : (
        <Icon size={Math.round(size * 0.94)} />
      )}
    </span>
  );
}

function getIconComponent(file: DocumentMetadata): React.ComponentType<{ className?: string; size?: number }> {
  if (file.web) {
    return Globe;
  }

  const ext = file.fileType || getFileExtension(file.fileName);

  // Documents
  if (['pdf', 'doc', 'docx', 'txt', 'md', 'markdown'].includes(ext)) {
    return FileText;
  }

  // Images
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'ico', 'bmp'].includes(ext)) {
    return Image;
  }

  // Code
  if (['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'html', 'css', 'json', 'xml', 'yml', 'yaml'].includes(ext)) {
    return FileCode;
  }

  // Archives
  if (['zip', 'rar', 'tar', 'gz', '7z'].includes(ext)) {
    return FileArchive;
  }

  // Video
  if (['mp4', 'mov', 'avi', 'mkv', 'webm', 'flv'].includes(ext)) {
    return FileVideo;
  }

  // Audio
  if (['mp3', 'wav', 'ogg', 'flac', 'aac', 'm4a'].includes(ext)) {
    return FileAudio;
  }

  // Spreadsheets
  if (['xlsx', 'xls', 'csv'].includes(ext)) {
    return FileSpreadsheet;
  }

  // Default
  return File;
}
