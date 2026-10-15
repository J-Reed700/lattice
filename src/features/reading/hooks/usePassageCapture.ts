import { useCallback, useState } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router';

import { PASSAGE_REFERENCES_QUERY_KEY } from '@/features/reading/hooks/usePassageReferencesQuery';
import VaultAPI from '@/lib/api';
import { formatSourceLocation } from '@/shared/sources/passageLocator';
import { toast } from '@/stores/toastStore';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';

interface PassageCaptureOptions {
  source: SourceWithMetadata;
  initialLocator: PassageLocator | null;
  /** The location a viewer resolved for the passage (e.g. a PDF page), if one has. */
  resolvedLabel: string | undefined;
  onClose: () => void;
}

/**
 * The reader's capture verbs for a passage — save it as a reference, quote it
 * into the journal, ask about it in a new chat — and the journal draft the
 * second one opens.
 */
export function usePassageCapture({
  source,
  initialLocator,
  resolvedLabel,
  onClose,
}: PassageCaptureOptions) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [captureDraft, setCaptureDraft] = useState<string | null>(null);

  const captureLocator = useCallback(
    (text: string) => text.trim().slice(0, 4000),
    []
  );

  const handleReference = useCallback(
    async (text: string) => {
      const trimmed = captureLocator(text);
      if (!trimmed) return;
      const result = await VaultAPI.createPassageReference({
        documentId: source.documentId,
        chunkId: initialLocator?.chunkId ?? source.chunkId,
        filePath: source.filePath,
        fileName: source.fileName,
        locator: resolvedLabel ?? formatSourceLocation(source) ?? undefined,
        text: trimmed,
      });
      if (!result.ok) {
        // Say what actually went wrong. "Not available yet" was true while the
        // slice was unbuilt and is a lie now that it ships.
        toast.error("Couldn't save this reference", { message: result.error });
        return;
      }
      await queryClient.invalidateQueries({ queryKey: PASSAGE_REFERENCES_QUERY_KEY });
      toast.success('Saved to references', {
        message: 'Lattice will use this in future answers.',
      });
    },
    [source, captureLocator, initialLocator?.chunkId, resolvedLabel, queryClient]
  );

  const handleAddToJournal = useCallback(
    async (text: string) => {
      const trimmed = captureLocator(text);
      if (!trimmed) return;
      const location = resolvedLabel ?? formatSourceLocation(source);
      const attribution = location
        ? `— ${source.fileName}, ${location}`
        : `— ${source.fileName}`;
      const quoted = trimmed
        .split('\n')
        .map((line) => `> ${line}`)
        .join('\n');
      setCaptureDraft(`${quoted}\n${attribution}`);
    },
    [source, captureLocator, resolvedLabel]
  );

  const handleAskAbout = useCallback(
    (text: string) => {
      const trimmed = captureLocator(text);
      if (!trimmed) return;
      onClose();
      navigate(
        `/chat?new=1&documentId=${encodeURIComponent(source.documentId)}` +
          `&quote=${encodeURIComponent(trimmed)}`
      );
    },
    [source, captureLocator, navigate, onClose]
  );

  return { captureDraft, setCaptureDraft, handleReference, handleAddToJournal, handleAskAbout };
}
