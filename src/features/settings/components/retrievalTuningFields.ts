import type { RetrievalTuningSettings as ApiRetrievalTuningSettings } from '@/types/api/settings';

import { clamp } from './searchSettingsOptions';


/// Only the numeric knobs get a number input. `sufficiencyRetryEnabled` is a
/// switch, and letting it into this union would type its stored value as
/// `number | boolean` everywhere the rows are rendered.
export type NumericTuningKey = {
  [K in keyof ApiRetrievalTuningSettings]: ApiRetrievalTuningSettings[K] extends number ? K : never;
}[keyof ApiRetrievalTuningSettings];

export type NumericFieldConfig = {
  key: NumericTuningKey;
  label: string;
  min: number;
  max: number;
  step: number;
};

export type TuningGroup = {
  title: string;
  fields: NumericFieldConfig[];
};

const CANDIDATE_FIELDS: NumericFieldConfig[] = [
  {
    key: 'docShortlistCandidateMin',
    label: 'Doc shortlist candidate min',
    min: 1,
    max: 512,
    step: 1,
  },
  {
    key: 'docShortlistCandidateMax',
    label: 'Doc shortlist candidate max',
    min: 1,
    max: 512,
    step: 1,
  },
  { key: 'docShortlistDocMin', label: 'Doc shortlist doc min', min: 1, max: 128, step: 1 },
  { key: 'docShortlistDocMax', label: 'Doc shortlist doc max', min: 1, max: 128, step: 1 },
  {
    key: 'shortlistGateMinCandidates',
    label: 'Shortlist gate min candidates',
    min: 1,
    max: 128,
    step: 1,
  },
  { key: 'shortlistGateMinDocs', label: 'Shortlist gate min docs', min: 1, max: 128, step: 1 },
];

const EXTERNAL_SOURCE_FIELDS: NumericFieldConfig[] = [
  { key: 'wikiSearchMaxResults', label: 'Wiki search max results', min: 1, max: 50, step: 1 },
  { key: 'wikiSnippetMaxChars', label: 'Wiki snippet max chars', min: 64, max: 4000, step: 1 },
  { key: 'wikiContextLimit', label: 'Wiki context limit', min: 1, max: 20, step: 1 },
  { key: 'webSearchMaxResults', label: 'Web search max results', min: 1, max: 50, step: 1 },
  { key: 'webSnippetMaxChars', label: 'Web snippet max chars', min: 64, max: 4000, step: 1 },
  {
    key: 'externalSearchMaxWikiTerms',
    label: 'External max wiki terms',
    min: 1,
    max: 64,
    step: 1,
  },
  {
    key: 'externalSearchMaxWebTerms',
    label: 'External max web terms',
    min: 1,
    max: 64,
    step: 1,
  },
  {
    key: 'externalSearchQueryMaxChars',
    label: 'External search query max chars',
    min: 32,
    max: 2000,
    step: 1,
  },
];

const DEEP_RESEARCH_FIELDS: NumericFieldConfig[] = [
  { key: 'deepResearchDepth', label: 'Deep research depth', min: 1, max: 4, step: 1 },
  {
    key: 'deepResearchBranchQueries',
    label: 'Deep research branch queries',
    min: 1,
    max: 4,
    step: 1,
  },
];

const RERANK_FIELDS: NumericFieldConfig[] = [
  { key: 'rerankMaxCandidates', label: 'Rerank max candidates', min: 1, max: 256, step: 1 },
  { key: 'rerankQueryMaxChars', label: 'Rerank query max chars', min: 32, max: 8000, step: 1 },
  {
    key: 'overlapMinHitsForMultiTerm',
    label: 'Overlap min hits (multi-term)',
    min: 1,
    max: 8,
    step: 1,
  },
];

const SUFFICIENCY_FIELDS: NumericFieldConfig[] = [
  {
    key: 'sufficiencyMinTopScore',
    label: 'Sufficiency min top score',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'sufficiencyMinTermCoverage',
    label: 'Sufficiency min term coverage',
    min: 0,
    max: 1,
    step: 0.01,
  },
];

const DOC_SUPPORT_FIELDS: NumericFieldConfig[] = [
  {
    key: 'docSupportMultiHitRatioFactor',
    label: 'Doc support multi-hit factor',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioFactor',
    label: 'Doc support single-hit factor',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportMultiHitRatioMin',
    label: 'Doc support multi-hit min',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportMultiHitRatioMax',
    label: 'Doc support multi-hit max',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioMin',
    label: 'Doc support single-hit min',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioMax',
    label: 'Doc support single-hit max',
    min: 0,
    max: 1,
    step: 0.01,
  },
];

export const TUNING_GROUPS: TuningGroup[] = [
  { title: 'Candidates and shortlist', fields: CANDIDATE_FIELDS },
  { title: 'External sources', fields: EXTERNAL_SOURCE_FIELDS },
  { title: 'Deep research', fields: DEEP_RESEARCH_FIELDS },
  { title: 'Rerank and overlap', fields: RERANK_FIELDS },
  { title: 'Corrective retrieval', fields: SUFFICIENCY_FIELDS },
  { title: 'Document support ratios', fields: DOC_SUPPORT_FIELDS },
];

export function normalizeFieldValue(field: NumericFieldConfig, value: number): number {
  const clamped = clamp(value, field.min, field.max);
  if (field.step >= 1) {
    return Math.round(clamped);
  }
  return Number(clamped.toFixed(2));
}

export function normalizeTuningPairBounds(
  key: NumericTuningKey,
  tuning: ApiRetrievalTuningSettings
): ApiRetrievalTuningSettings {
  const next = { ...tuning };

  if (
    key === 'docShortlistCandidateMin' &&
    next.docShortlistCandidateMin > next.docShortlistCandidateMax
  ) {
    next.docShortlistCandidateMax = next.docShortlistCandidateMin;
  }
  if (
    key === 'docShortlistCandidateMax' &&
    next.docShortlistCandidateMax < next.docShortlistCandidateMin
  ) {
    next.docShortlistCandidateMin = next.docShortlistCandidateMax;
  }

  if (key === 'docShortlistDocMin' && next.docShortlistDocMin > next.docShortlistDocMax) {
    next.docShortlistDocMax = next.docShortlistDocMin;
  }
  if (key === 'docShortlistDocMax' && next.docShortlistDocMax < next.docShortlistDocMin) {
    next.docShortlistDocMin = next.docShortlistDocMax;
  }

  if (
    key === 'docSupportMultiHitRatioMin' &&
    next.docSupportMultiHitRatioMin > next.docSupportMultiHitRatioMax
  ) {
    next.docSupportMultiHitRatioMax = next.docSupportMultiHitRatioMin;
  }
  if (
    key === 'docSupportMultiHitRatioMax' &&
    next.docSupportMultiHitRatioMax < next.docSupportMultiHitRatioMin
  ) {
    next.docSupportMultiHitRatioMin = next.docSupportMultiHitRatioMax;
  }

  if (
    key === 'docSupportSingleHitRatioMin' &&
    next.docSupportSingleHitRatioMin > next.docSupportSingleHitRatioMax
  ) {
    next.docSupportSingleHitRatioMax = next.docSupportSingleHitRatioMin;
  }
  if (
    key === 'docSupportSingleHitRatioMax' &&
    next.docSupportSingleHitRatioMax < next.docSupportSingleHitRatioMin
  ) {
    next.docSupportSingleHitRatioMin = next.docSupportSingleHitRatioMax;
  }

  return next;
}
