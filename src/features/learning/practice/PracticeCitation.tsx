import { useState } from 'react';

import { BookOpen } from 'lucide-react';

import { Button } from '@/components/ui';
import { SourceReader } from '@/features/learning/sources/SourceReader';
import { useLearningSourceVersion } from '@/features/learning/sources/useLearningSources';
import type { LearningPracticeCitationDto } from '@/lib/bindings';

/** Inspect the immutable version behind feedback, without fetching its live URL. */
export function PracticeCitation({ programId, citation, canOpen = true }: {
  programId: string;
  citation: LearningPracticeCitationDto;
  canOpen?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const available = canOpen && Boolean(citation.sourceId && citation.versionId);
  const source = useLearningSourceVersion(open && available
    ? { programId, sourceId: citation.sourceId, versionId: citation.versionId }
    : null);

  return <div className="mt-2 border-l-2 border-accent/40 pl-3">
    <blockquote className="text-xs italic leading-5 text-text-muted">“{citation.quote}”</blockquote>
    {available && <Button variant="ghost" size="sm" className="mt-1 text-xs text-accent" aria-expanded={open} onClick={() => setOpen((value) => !value)}>
      <BookOpen size={13} aria-hidden="true" /> {open ? 'Close saved source' : 'View saved source'}
    </Button>}
    {open && available && <div className="mt-2 text-xs">
      {source.isLoading && <p role="status" className="text-text-muted">Opening the cited version…</p>}
      {source.isError && <div><p role="alert" className="text-text-secondary">{source.error.message}</p><Button variant="ghost" size="sm" onClick={() => void source.refetch()}>Retry source</Button></div>}
      {source.data && <>
        <p className="font-semibold text-text-primary">{source.data.version.title}</p>
        <p className="mt-1 text-text-muted">Version {source.data.version.versionNumber}{source.data.version.publisher ? ` · ${source.data.version.publisher}` : ''}</p>
        {source.data.version.resolvedUrl && <p className="mt-1 break-all text-text-muted">{source.data.version.resolvedUrl}</p>}
        <SourceReader fullText={source.data.fullText} extractionVersion={source.data.version.extractionVersion} wordCount={source.data.version.wordCount} />
      </>}
    </div>}
  </div>;
}
