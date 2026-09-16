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

export function FileIcon({ file, className = '', size = 16 }: FileIconProps) {
  const Icon = getIconComponent(file);
  const color = getIconColor(file);

  return <Icon className={`${className} ${color}`} size={size} />;
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

function getIconColor(file: DocumentMetadata): string {
  // Web articles pick up the accent; every document type stays quiet so
  // status colors (danger/warning/success) keep their meaning.
  if (file.filePath.includes('/.lattice/web-archive/')) {
    return 'text-[hsl(var(--accent))]';
  }

  return 'text-[hsl(var(--text-secondary))]';
}
