import { FileQuestion } from 'lucide-react';

import { Button } from '@/components/ui/button';

interface UnsupportedViewerProps {
  filePath: string;
  onOpenExternal: () => void;
}

export function UnsupportedViewer({ onOpenExternal }: UnsupportedViewerProps) {
  return (
    <div className="flex flex-col items-center justify-center h-full p-6 text-center">
      <FileQuestion className="h-16 w-16 text-muted-foreground mb-4" />
      <h3 className="text-lg font-semibold mb-2">Preview Not Available</h3>
      <p className="text-muted-foreground mb-6 max-w-md">
        This file type cannot be previewed in Lattice.
        You can open it in your system's default application instead.
      </p>
      <Button onClick={onOpenExternal}>
        Open in System Viewer
      </Button>
    </div>
  );
}
