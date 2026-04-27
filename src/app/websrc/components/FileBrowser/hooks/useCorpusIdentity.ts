import { useMemo } from 'react';

import { type DocumentMetadata } from '../../../types/fileBrowser';

export type IdentityRenderMode = 'empty' | 'nascent' | 'full';

export interface TypeBucket {
  key: string;
  label: string;
  count: number;
  percentage: number;
}

export interface CorpusIdentity {
  mode: IdentityRenderMode;
  totalCount: number;
  ledeCount: string;
  subline: string | null;
  buckets: TypeBucket[];
  legend: string;
}

const normalizeToken = (value: string): string => value.trim().toLowerCase();
const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);

const isWebDocument = (doc: DocumentMetadata): boolean => {
  if (isHttpUrl(doc.filePath)) return true;
  if (doc.filePath.includes('/.lattice/web-archive/')) return true;
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  return normalizedCategory.includes('web article') || normalizedCategory === 'web';
};

const categorize = (doc: DocumentMetadata): string => {
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  const normalizedFileType = normalizeToken(doc.fileType ?? '');

  if (isWebDocument(doc)) return 'Web';
  if (normalizedCategory.includes('pdf') || normalizedFileType === 'pdf') return 'PDFs';
  if (['md', 'markdown', 'txt', 'rtf'].includes(normalizedFileType)) return 'Notes';
  if (['doc', 'docx'].includes(normalizedFileType)) return 'Documents';
  if (['js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h', 'html', 'css', 'json', 'xml', 'yml', 'yaml'].includes(normalizedFileType)) return 'Code';
  if (['mp3', 'wav', 'flac', 'ogg', 'm4a', 'mp4', 'mkv', 'mov', 'avi', 'webm'].includes(normalizedFileType)) return 'Media';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(normalizedFileType)) return 'Images';
  if (['zip', 'rar', '7z', 'tar', 'gz'].includes(normalizedFileType)) return 'Archives';
  if (['xlsx', 'xls', 'csv'].includes(normalizedFileType)) return 'Spreadsheets';
  return 'Other';
};

const formatCount = (value: number): string => value.toLocaleString();

const pluralize = (count: number, singular: string, plural: string): string =>
  count === 1 ? singular : plural;

const toSublineNoun = (label: string): string => {
  switch (label) {
    case 'PDFs': return 'PDFs';
    case 'Web': return 'web clippings';
    case 'Notes': return 'notes';
    case 'Documents': return 'documents';
    case 'Code': return 'code files';
    case 'Media': return 'media';
    case 'Images': return 'images';
    case 'Archives': return 'archives';
    case 'Spreadsheets': return 'spreadsheets';
    default: return 'other files';
  }
};

const composeSubline = (top: TypeBucket[]): string => {
  const relevant = top.filter((bucket) => bucket.count > 0).slice(0, 3);
  if (relevant.length === 0) return '';
  if (relevant.length === 1) {
    return `${toSublineNoun(relevant[0].label)}, mostly.`;
  }
  if (relevant.length === 2) {
    return `${toSublineNoun(relevant[0].label)} and ${toSublineNoun(relevant[1].label)}, mostly.`;
  }
  return `${toSublineNoun(relevant[0].label)}, ${toSublineNoun(relevant[1].label)}, and ${toSublineNoun(relevant[2].label)}, mostly.`;
};

export function useCorpusIdentity(documents: DocumentMetadata[]): CorpusIdentity {
  return useMemo(() => {
    const totalCount = documents.length;

    if (totalCount === 0) {
      return {
        mode: 'empty',
        totalCount: 0,
        ledeCount: 'An empty lattice — waiting.',
        subline: null,
        buckets: [],
        legend: '',
      };
    }

    const bucketMap = new Map<string, number>();
    for (const doc of documents) {
      const label = categorize(doc);
      bucketMap.set(label, (bucketMap.get(label) ?? 0) + 1);
    }

    const buckets: TypeBucket[] = Array.from(bucketMap.entries())
      .map(([label, count]) => ({
        key: label,
        label,
        count,
        percentage: Math.round((count / totalCount) * 1000) / 10,
      }))
      .sort((a, b) => b.count - a.count);

    if (totalCount < 10) {
      const noun = pluralize(totalCount, 'document', 'documents');
      return {
        mode: 'nascent',
        totalCount,
        ledeCount: `A nascent lattice. ${formatCount(totalCount)} ${noun}.`,
        subline: null,
        buckets: [],
        legend: '',
      };
    }

    const legend = buckets
      .slice(0, 5)
      .map((bucket) => `${bucket.label} ${formatCount(bucket.count)}`)
      .join(' · ');

    return {
      mode: 'full',
      totalCount,
      ledeCount: `${formatCount(totalCount)} ${pluralize(totalCount, 'document', 'documents')}.`,
      subline: composeSubline(buckets),
      buckets,
      legend,
    };
  }, [documents]);
}
