import { useMemo } from 'react';

import { provenanceLabel, sourceProvenance } from '@/features/chat/model/sourceProvenance';
import { useCitationInteractions } from '@/features/reading/hooks/useCitationInteractions';
import { usePassageReferenceIds } from '@/features/reading/hooks/usePassageReferencesQuery';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import type { ClaimVerdict, SourceWithMetadata } from '@/types/conversation';

interface MessageEvidenceOptions {
  sources: SourceWithMetadata[];
  ownerKey: string;
  isAssistantWithSources: boolean;
  showCitations: boolean;
  claimVerdicts: ClaimVerdict[];
  showsCodeRefs: boolean;
  revealInExplorer: (path: string, range: { startLine: number; endLine: number }) => void;
  openInExplorer: (path: string) => void;
}

/** Owns citation navigation, provenance and the answer's delegated interactions. */
export function useMessageEvidence({
  sources, ownerKey, isAssistantWithSources, showCitations, claimVerdicts,
  showsCodeRefs, revealInExplorer, openInExplorer,
}: MessageEvidenceOptions) {
  const settings = useSettingsQuery().data;
  const referenceKeys = usePassageReferenceIds();
  const interactions = useCitationInteractions({
    sources,
    ownerKey,
    numbered: isAssistantWithSources,
    showCitations,
    claims: claimVerdicts,
    codeRefs: showsCodeRefs ? { reveal: revealInExplorer, open: openInExplorer } : undefined,
  });

  const vaultPath = settings?.vault?.vaultPath ?? '';
  const provenanceBySource = useMemo(() => {
    const map = new Map<string, string>();
    for (const source of sources) {
      const label = provenanceLabel(sourceProvenance(source, vaultPath, referenceKeys));
      if (label) map.set(source.chunkId, label);
    }
    return map;
  }, [sources, vaultPath, referenceKeys]);

  const { claimHover } = interactions;
  const hoveredVerdict = claimHover ? claimVerdicts[claimHover.index] ?? null : null;

  return { ...interactions, provenanceBySource, hoveredVerdict };
}
