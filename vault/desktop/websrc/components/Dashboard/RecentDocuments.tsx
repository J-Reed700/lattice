import { FileText, ExternalLink, File, FileImage, FileCode, Clock } from 'lucide-react';

import { sanitizeFileName } from '@/utils/sanitize';

import VaultAPI from '../../lib/api';
import Card, { CardHeader, CardTitle, CardContent } from '../ui/Card/Card';

import type { RecentDocument } from '../../types';


/**
 * RecentDocuments
 *
 * Purpose: Display recently indexed documents with quick access
 *
 * Features:
 * - List of last 10 documents
 * - File type icons
 * - File size and date information
 * - Click to open in default application
 * - Empty state for no documents
 * - Loading skeleton states
 *
 * States: loading, empty, populated
 * Accessibility: WCAG AA, keyboard navigation, screen reader support
 */

interface RecentDocumentsProps {
  documents: RecentDocument[];
  loading?: boolean;
}

export const RecentDocuments = ({ documents, loading = false }: RecentDocumentsProps) => {
  const getFileIcon = (fileType: string | null) => {
    if (!fileType) return File;

    const type = fileType.toLowerCase();
    if (type.includes('image') || ['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(type)) {
      return FileImage;
    }
    if (['js', 'ts', 'tsx', 'jsx', 'py', 'rs', 'go', 'java'].includes(type)) {
      return FileCode;
    }
    return FileText;
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    const kb = bytes / 1024;
    if (kb < 1024) return `${kb.toFixed(1)} KB`;
    const mb = kb / 1024;
    if (mb < 1024) return `${mb.toFixed(1)} MB`;
    const gb = mb / 1024;
    return `${gb.toFixed(2)} GB`;
  };

  const getRelativeTime = (timestamp: string): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;

    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;

    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;

    return date.toLocaleDateString();
  };

  const handleOpenDocument = async (filePath: string) => {
    const result = await VaultAPI.openFile(filePath);
    if (!result.ok) {
      console.error('Failed to open file:', result.error);
    }
  };

  if (loading) {
    return (
      <Card padding="md" className="h-full">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <FileText className="w-5 h-5" />
            Recent Documents
          </CardTitle>
        </CardHeader>
        <CardContent className="mt-4 space-y-3">
          {[...Array(5)].map((_, i) => (
            <div key={i} className="animate-pulse flex items-start gap-3">
              <div className="w-10 h-10 bg-[var(--bg-tertiary)] rounded" />
              <div className="flex-1 space-y-2">
                <div className="h-4 bg-[var(--bg-tertiary)] rounded w-3/4" />
                <div className="h-3 bg-[var(--bg-tertiary)] rounded w-1/2" />
              </div>
            </div>
          ))}
        </CardContent>
      </Card>
    );
  }

  return (
    <Card padding="md" className="h-full flex flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-[var(--text-primary)]">
          <FileText className="w-5 h-5 text-[var(--accent-primary)]" />
          Recent Documents
        </CardTitle>
      </CardHeader>

      <CardContent className="mt-4 flex-1 overflow-auto">
        {documents.length === 0 ? (
          <div className="text-center py-8">
            <Clock className="w-12 h-12 text-[var(--text-tertiary)] mx-auto mb-3" />
            <p className="text-[var(--text-secondary)] text-sm">
              No documents yet
            </p>
            <p className="text-[var(--text-tertiary)] text-xs mt-1">
              Index a folder to get started
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {documents.map((doc) => {
              const FileIcon = getFileIcon(doc.fileType);

              return (
                <button
                  key={doc.id}
                  onClick={() => handleOpenDocument(doc.filePath)}
                  className="w-full flex items-start gap-3 p-3 rounded-lg hover:bg-[var(--bg-tertiary)] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-primary)] focus-visible:ring-offset-2 group"
                  aria-label={`Open ${sanitizeFileName(doc.fileName)}`}
                >
                  {/* File Icon */}
                  <div className="flex-shrink-0 w-10 h-10 bg-[var(--surface-elevated)] rounded flex items-center justify-center group-hover:bg-[var(--accent-light)] transition-colors">
                    <FileIcon className="w-5 h-5 text-[var(--text-secondary)] group-hover:text-[var(--accent-primary)]" />
                  </div>

                  {/* File Info */}
                  <div className="flex-1 min-w-0 text-left">
                    <div className="flex items-center gap-2">
                      <p className="text-sm font-medium text-[var(--text-primary)] truncate">
                        {sanitizeFileName(doc.fileName)}
                      </p>
                      <ExternalLink className="w-3 h-3 text-[var(--text-tertiary)] opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                    </div>

                    <div className="flex items-center gap-3 mt-1 text-xs text-[var(--text-secondary)]">
                      <span>{formatFileSize(doc.sizeBytes)}</span>
                      <span>•</span>
                      <span>{getRelativeTime(doc.indexedAt)}</span>
                      {doc.fileType && (
                        <>
                          <span>•</span>
                          <span className="uppercase">{doc.fileType}</span>
                        </>
                      )}
                    </div>

                    <p
                      className="text-xs text-[var(--text-tertiary)] mt-1 truncate"
                      title={doc.filePath}
                    >
                      {doc.filePath}
                    </p>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default RecentDocuments;
