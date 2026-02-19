import {
  Globe,
  FileText,
  FileCode,
  Image,
  FileArchive,
  FileVideo,
  FileAudio,
  FileSpreadsheet,
  File,
  type LucideIcon
} from 'lucide-react';

import { type FileNode, type DocumentMetadata } from '../../types/fileBrowser';

interface FileTypeBadgeProps {
  file: FileNode | DocumentMetadata;
  size?: 'sm' | 'md' | 'lg';
  showIcon?: boolean;
  showLabel?: boolean;
}

interface FileTypeInfo {
  icon: LucideIcon;
  label: string;
  color: string;
  bgColor: string;
}

export function FileTypeBadge({
  file,
  size = 'sm',
  showIcon = true,
  showLabel = true
}: FileTypeBadgeProps) {
  const typeInfo = getFileTypeInfo(file);

  const sizeClasses = {
    sm: 'px-2 py-0.5 text-xs',
    md: 'px-3 py-1 text-sm',
    lg: 'px-4 py-1.5 text-base',
  };

  const iconSizes = {
    sm: 12,
    md: 14,
    lg: 16,
  };

  const Icon = typeInfo.icon;

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full font-medium ${sizeClasses[size]} ${typeInfo.bgColor} ${typeInfo.color}`}
    >
      {showIcon && <Icon size={iconSizes[size]} />}
      {showLabel && <span>{typeInfo.label}</span>}
    </span>
  );
}

function getFileExtension(fileName: string): string {
  const parts = fileName.split('.');
  return parts.length > 1 ? parts.pop()!.toLowerCase() : '';
}

function getFileTypeInfo(file: FileNode | DocumentMetadata): FileTypeInfo {
  // Get path from either FileNode or DocumentMetadata
  const path = 'filePath' in file ? file.filePath : file.path;

  // Web archive files get special treatment
  if (path.includes('/.recall/web-archive/')) {
    return {
      icon: Globe,
      label: 'Web Article',
      color: 'text-blue-700',
      bgColor: 'bg-blue-100',
    };
  }

  // Get name and extension
  const name = 'fileName' in file ? file.fileName : file.name;
  const fileExt = 'fileType' in file ? file.fileType : file.extension;
  const ext = fileExt || getFileExtension(name);

  // Documents
  if (ext === 'pdf') {
    return {
      icon: FileText,
      label: 'PDF Document',
      color: 'text-red-700',
      bgColor: 'bg-red-100',
    };
  }

  if (['doc', 'docx'].includes(ext)) {
    return {
      icon: FileText,
      label: 'Word Document',
      color: 'text-blue-700',
      bgColor: 'bg-blue-100',
    };
  }

  if (['txt', 'md', 'markdown'].includes(ext)) {
    return {
      icon: FileText,
      label: 'Text Document',
      color: 'text-gray-700',
      bgColor: 'bg-gray-100',
    };
  }

  // Images
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp'].includes(ext)) {
    return {
      icon: Image,
      label: 'Image',
      color: 'text-purple-700',
      bgColor: 'bg-purple-100',
    };
  }

  // Code files
  if (['js', 'jsx', 'ts', 'tsx'].includes(ext)) {
    return {
      icon: FileCode,
      label: 'JavaScript',
      color: 'text-yellow-700',
      bgColor: 'bg-yellow-100',
    };
  }

  if (ext === 'py') {
    return {
      icon: FileCode,
      label: 'Python',
      color: 'text-blue-700',
      bgColor: 'bg-blue-100',
    };
  }

  if (ext === 'rs') {
    return {
      icon: FileCode,
      label: 'Rust',
      color: 'text-orange-700',
      bgColor: 'bg-orange-100',
    };
  }

  if (['html', 'css'].includes(ext)) {
    return {
      icon: FileCode,
      label: ext.toUpperCase(),
      color: 'text-pink-700',
      bgColor: 'bg-pink-100',
    };
  }

  if (['json', 'xml', 'yml', 'yaml', 'toml'].includes(ext)) {
    return {
      icon: FileCode,
      label: 'Config',
      color: 'text-green-700',
      bgColor: 'bg-green-100',
    };
  }

  // Archives
  if (['zip', 'rar', 'tar', 'gz', '7z'].includes(ext)) {
    return {
      icon: FileArchive,
      label: 'Archive',
      color: 'text-amber-700',
      bgColor: 'bg-amber-100',
    };
  }

  // Video
  if (['mp4', 'mov', 'avi', 'mkv', 'webm'].includes(ext)) {
    return {
      icon: FileVideo,
      label: 'Video',
      color: 'text-indigo-700',
      bgColor: 'bg-indigo-100',
    };
  }

  // Audio
  if (['mp3', 'wav', 'ogg', 'flac'].includes(ext)) {
    return {
      icon: FileAudio,
      label: 'Audio',
      color: 'text-cyan-700',
      bgColor: 'bg-cyan-100',
    };
  }

  // Spreadsheets
  if (['xlsx', 'xls', 'csv'].includes(ext)) {
    return {
      icon: FileSpreadsheet,
      label: 'Spreadsheet',
      color: 'text-green-700',
      bgColor: 'bg-green-100',
    };
  }

  // Default
  return {
    icon: File,
    label: ext ? ext.toUpperCase() : 'File',
    color: 'text-gray-700',
    bgColor: 'bg-gray-100',
  };
}
