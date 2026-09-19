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

import { type DocumentMetadata, getFileExtension } from '../../types/fileBrowser';

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

/**
 * A small format tile. Documents are told apart by their label ("PDF", "MD"),
 * not by colour: colour stays reserved for state. Formats without a familiar
 * label fall back to a glyph.
 */
export function FileIcon({ file, className = '', size = 16 }: FileIconProps) {
  const isWeb = file.filePath.includes('/.lattice/web-archive/');
  const ext = (file.fileType || getFileExtension(file.fileName) || '').toLowerCase();
  const label = isWeb ? null : FORMAT_LABELS[ext];
  const tile = Math.round(size * 1.75);
  const Icon = getIconComponent(file);

  return (
    <span
      aria-hidden="true"
      style={{ width: tile, height: tile }}
      className={`inline-flex items-center justify-center rounded-md ${
        isWeb ? 'bg-accent-muted text-accent' : 'bg-[hsl(var(--text-primary)/0.06)] text-text-tertiary'
      } ${className}`}
    >
      {label ? (
        <span
          className="font-sans font-semibold leading-none tracking-[0.02em]"
          style={{ fontSize: label.length > 3 ? Math.max(7, size * 0.44) : Math.max(8, size * 0.53) }}
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
  // Web archive files get Globe icon
  if (file.filePath.includes('/.lattice/web-archive/')) {
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
