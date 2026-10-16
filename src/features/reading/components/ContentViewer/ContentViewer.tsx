import { useMemo } from 'react';

import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import { fromFilePath } from '@/features/reading/model/sourceAdapter';

interface ContentViewerProps {
  filePath: string | null;
  onClose: () => void;
}

export function ContentViewer({ filePath, onClose }: ContentViewerProps) {
  const target = useMemo(
    () => (filePath ? fromFilePath(filePath) : null),
    [filePath],
  );
  return (
    <FilePreviewModal
      isOpen={target !== null}
      onClose={onClose}
      source={target?.source ?? null}
      initialLocator={target?.locator ?? null}
    />
  );
}
