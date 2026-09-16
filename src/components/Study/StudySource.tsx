import { useState } from 'react';

import { ContentViewer } from '@/components/ContentViewer/ContentViewer';
import { Button } from '@/components/ui/button';
import type { StudySourceDto } from '@/lib/bindings';

export function StudySource({ sources }: { sources: StudySourceDto[] }) {
  const [openIndex, setOpenIndex] = useState<number | null>(null);
  if (!sources.length) return null;
  return (
    <details className="mt-5 border-t border-border-subtle pt-4 text-sm">
      <summary className="cursor-pointer text-text-secondary">{sources.length === 1 ? `Citation · ${sources[0].fileName}` : `Citations · ${sources.length} sources`}</summary>
      <div className="space-y-5 py-4">
        {sources.map((source, index) => <div key={`${source.chunkId}-${index}`}>
          {sources.length > 1 && <p className="mb-2 text-xs font-medium text-text-primary">[{index + 1}] {source.fileName}</p>}
          <blockquote className="border-l-2 border-accent pl-4 leading-relaxed text-text-secondary">{source.excerpt}</blockquote>
          <Button className="mt-3" variant="secondary" size="sm" onClick={() => setOpenIndex(index)}>Open source</Button>
        </div>)}
      </div>
      {openIndex !== null && sources[openIndex] && <ContentViewer filePath={sources[openIndex].filePath} onClose={() => setOpenIndex(null)} />}
    </details>
  );
}
