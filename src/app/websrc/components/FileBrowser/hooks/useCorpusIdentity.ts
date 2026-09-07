import { useMemo } from 'react';

import { type DocumentMetadata } from '../../../types/fileBrowser';
import { typeBucket } from '../docMeta';


export interface TypeBucket {
  label: string;
  count: number;
}

export interface CorpusIdentity {
  totalCount: number;
  buckets: TypeBucket[];
}

/**
 * The corpus readout: what this vault is made of, as counts per type.
 *
 * Computed from the same array the Library filters, so the facet counts and the
 * facet filter can never disagree; `get_corpus_shape` serves the surfaces that
 * do not already hold the document list.
 */
export function useCorpusIdentity(documents: DocumentMetadata[]): CorpusIdentity {
  return useMemo(() => {
    const counts = new Map<string, number>();
    for (const doc of documents) {
      const label = typeBucket(doc);
      counts.set(label, (counts.get(label) ?? 0) + 1);
    }

    const buckets = Array.from(counts.entries())
      .map(([label, count]) => ({ label, count }))
      .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));

    return { totalCount: documents.length, buckets };
  }, [documents]);
}
