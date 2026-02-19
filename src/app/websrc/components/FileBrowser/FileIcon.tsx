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
  Folder,
  FolderOpen,
  Globe,
} from 'lucide-react';

import { type FileNode, type DocumentMetadata, getFileExtension } from '../../types/fileBrowser';

interface FileIconProps {
  file: FileNode | DocumentMetadata;
  className?: string;
  size?: number;
}

export function FileIcon({ file, className = '', size = 16 }: FileIconProps) {
  const Icon = getIconComponent(file);
  const color = getIconColor(file);

  return <Icon className={`${className} ${color}`} size={size} />;
}

function getIconComponent(file: FileNode | DocumentMetadata): React.ComponentType<{ className?: string; size?: number }> {
  // Check if it's a FileNode with directory type
  if ('type' in file && file.type === 'directory') {
    return (file as FileNode).isExpanded ? FolderOpen : Folder;
  }

  // Get path from either FileNode or DocumentMetadata
  const path = 'filePath' in file ? file.filePath : file.path;

  // Web archive files get Globe icon
  if (path.includes('/.recall/web-archive/')) {
    return Globe;
  }

  // Get name and extension
  const name = 'fileName' in file ? file.fileName : file.name;
  const fileExt = 'fileType' in file ? file.fileType : file.extension;
  const ext = fileExt || getFileExtension(name);

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

function getIconColor(file: FileNode | DocumentMetadata): string {
  // Check if it's a directory
  if ('type' in file && file.type === 'directory') {
    return 'text-[var(--accent-primary)]';
  }

  // Get path from either FileNode or DocumentMetadata
  const path = 'filePath' in file ? file.filePath : file.path;

  // Web archive files - blue globe theme
  if (path.includes('/.recall/web-archive/')) {
    return 'text-blue-600';
  }

  // Get name and extension
  const name = 'fileName' in file ? file.fileName : file.name;
  const fileExt = 'fileType' in file ? file.fileType : file.extension;
  const ext = fileExt || getFileExtension(name);

  // Documents
  if (['pdf', 'doc', 'docx'].includes(ext)) {
    return 'text-[var(--error)]';
  }

  // Text
  if (['txt', 'md', 'markdown'].includes(ext)) {
    return 'text-[var(--text-secondary)]';
  }

  // Images
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(ext)) {
    return 'text-purple-500';
  }

  // Code
  if (['js', 'jsx', 'ts', 'tsx'].includes(ext)) {
    return 'text-[var(--warning)]';
  }

  if (['py'].includes(ext)) {
    return 'text-[var(--accent-primary)]';
  }

  if (['rs'].includes(ext)) {
    return 'text-orange-500';
  }

  if (['html', 'css'].includes(ext)) {
    return 'text-pink-500';
  }

  if (['json', 'xml', 'yml', 'yaml'].includes(ext)) {
    return 'text-[var(--success)]';
  }

  // Archives
  if (['zip', 'rar', 'tar', 'gz', '7z'].includes(ext)) {
    return 'text-amber-500';
  }

  // Media
  if (['mp4', 'mov', 'avi', 'mkv', 'webm'].includes(ext)) {
    return 'text-indigo-500';
  }

  if (['mp3', 'wav', 'ogg', 'flac'].includes(ext)) {
    return 'text-cyan-500';
  }

  // Spreadsheets
  if (['xlsx', 'xls', 'csv'].includes(ext)) {
    return 'text-[var(--success)]';
  }

  // Default
  return 'text-[var(--text-secondary)]';
}
