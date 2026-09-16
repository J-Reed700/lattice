import { useEffect, useState } from 'react';

import { Loader2 } from 'lucide-react';

import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { useFileContent } from '@/hooks/useFileContent';
import VaultAPI from '@/lib/api';
import { detectFileType } from '@/utils/fileTypeDetector';

import { CodeViewer } from './renderers/CodeViewer';
import { DocxViewer } from './renderers/DocxViewer';
import { HTMLViewer } from './renderers/HTMLViewer';
import { ImageViewer } from './renderers/ImageViewer';
import { MarkdownViewer } from './renderers/MarkdownViewer';
import { PDFViewer } from './renderers/PDFViewer';
import { TextViewer } from './renderers/TextViewer';
import { UnsupportedViewer } from './renderers/UnsupportedViewer';
import { ViewerHeader } from './ViewerHeader';


interface ContentViewerProps {
  filePath: string | null;
  onClose: () => void;
}

export function ContentViewer({ filePath, onClose }: ContentViewerProps) {
  const isOpen = filePath !== null;
  const fileTypeInfo = filePath ? detectFileType(filePath) : null;
  const canReadAsText = Boolean(
    fileTypeInfo?.type === 'markdown' ||
    fileTypeInfo?.type === 'code' ||
    fileTypeInfo?.type === 'text'
  );
  const shouldFetchContent = Boolean(filePath && canReadAsText);
  const { content, isLoading, error } = useFileContent(
    shouldFetchContent ? filePath || undefined : undefined,
    shouldFetchContent
  );

  const [viewMode, setViewMode] = useState<'preview' | 'internal'>('preview');
  const [openFileResponse, setOpenFileResponse] = useState<{
    file_type: string;
    content_path: string;
    title?: string;
  } | null>(null);

  const fileName = filePath ? filePath.split('/').pop() || filePath : '';

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    if (isOpen) {
      window.addEventListener('keydown', handleEscape);
      return () => window.removeEventListener('keydown', handleEscape);
    }
  }, [isOpen, onClose]);

  const handleOpenExternal = async () => {
    if (filePath) {
      const result = await VaultAPI.openFile(filePath);
      if (!result.ok) {
        console.error('Failed to open file:', result.error);
        return;
      }

      if (result.data.action === 'render_internal') {
        // Show appropriate viewer inline (don't close dialog)
        setViewMode('internal');
        setOpenFileResponse({
          file_type: result.data.fileType,
          content_path: result.data.contentPath,
          title: result.data.title ?? undefined,
        });
      } else {
        // File opened externally, close viewer
        onClose();
      }
    }
  };

  const renderContent = () => {
    // If in internal view mode, show appropriate viewer based on file type
    if (viewMode === 'internal' && openFileResponse) {
      const { file_type, content_path, title } = openFileResponse;

      switch (file_type) {
        case 'web_article_html':
          return <HTMLViewer htmlPath={content_path} title={title} showTitle={false} />;
        case 'pdf':
          return <PDFViewer filePath={content_path} title={title} />;
        case 'image':
          return <ImageViewer filePath={content_path} title={title} />;
        case 'text':
          return <TextViewer filePath={content_path} title={title} />;
        default:
          // Unknown file type - close dialog
          onClose();
          return null;
      }
    }

    if (isLoading) {
      return (
        <div className="flex items-center justify-center h-full">
          <Loader2 className="h-8 w-8 animate-spin text-[hsl(var(--text-secondary))]" />
        </div>
      );
    }

    if (error) {
      return (
        <div className="flex items-center justify-center h-full p-6 text-center">
          <div>
            <p className="text-[hsl(var(--danger))] font-semibold mb-2">Failed to load file</p>
            <p className="text-sm text-[hsl(var(--text-secondary))]">{error}</p>
          </div>
        </div>
      );
    }

    if (!fileTypeInfo?.canPreview) {
      return (
        <UnsupportedViewer
          filePath={filePath || ''}
          onOpenExternal={handleOpenExternal}
        />
      );
    }

    switch (fileTypeInfo.type) {
      case 'markdown':
        return <MarkdownViewer content={content} />;
      case 'code':
        return <CodeViewer content={content} language={fileTypeInfo.language} />;
      case 'text':
        return <TextViewer content={content} />;
      case 'html':
        return <HTMLViewer htmlPath={filePath || ''} title={fileName} showTitle={false} />;
      case 'docx':
        return <DocxViewer filePath={filePath || ''} title={fileName} />;
      case 'pdf':
        return <PDFViewer filePath={filePath || ''} title={fileName} />;
      case 'image':
        return <ImageViewer filePath={filePath || ''} title={fileName} />;
      default:
        return (
          <UnsupportedViewer
            filePath={filePath || ''}
            onOpenExternal={handleOpenExternal}
          />
        );
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-5xl h-[85vh] p-0 gap-0 !flex !flex-col" aria-describedby={undefined}>
        <DialogTitle className="sr-only">{fileName}</DialogTitle>
        <ViewerHeader fileName={fileName} />
        <div className="flex-1 overflow-hidden">
          {renderContent()}
        </div>
      </DialogContent>
    </Dialog>
  );
}
