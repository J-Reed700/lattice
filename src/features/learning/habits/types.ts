export type FrameworkAction =
  | 'recall-dump'
  | 'spaced-repetition'
  | 'feynman'
  | 'interleaving'
  | 'dual-coding'
  | 'handwritten-notes';

export interface RetentionFramework {
  id: FrameworkAction;
  title: string;
  badge: string;
  retentionStat: string;
  description: string;
  actionLabel: string;
  keyPrinciples: readonly string[];
  cognitiveBenefit: string;
}

export interface ChecklistItem {
  id: string;
  label: string;
  category: 'cognition' | 'health' | 'metacognition';
  evidenceSource: string;
  summary: string;
}

export type HabitChecklistState = Record<string, boolean>;

export const HABIT_CHECKLIST_STORAGE_KEY = 'lattice.learning.habits.checklist.v1';

export const CHECKLIST_ITEMS: readonly ChecklistItem[] = [
  {
    id: 'focused-block',
    label: 'Focused block: Studying in a 30–45 minute undistracted block (no multitasking or cramming)',
    category: 'cognition',
    evidenceSource: 'Harvard Retention Principles & Cognitive Load Theory',
    summary: 'Single-task focus preserves working memory bandwidth and prevents cognitive exhaustion.',
  },
  {
    id: 'pre-study-recall',
    label: 'Pre-study recall: Tested what I already know before rereading',
    category: 'metacognition',
    evidenceSource: 'Karpicke & Roediger (2008) Retrieval Practice',
    summary: 'Pre-testing primes neural circuits and exposes knowledge blindspots prior to reading.',
  },
  {
    id: 'own-words-synthesis',
    label: 'Own-words synthesis: Rephrased key takeaways without copying word-for-word',
    category: 'cognition',
    evidenceSource: 'Generative Learning (Wittrock & Fiorella)',
    summary: 'Reformulating ideas in personal terminology converts passive text into durable long-term mental models.',
  },
  {
    id: 'sleep-consolidation',
    label: 'Sleep & consolidation: Slept 7–8+ hours for synaptic memory consolidation',
    category: 'health',
    evidenceSource: 'Mayo Clinic & Stickgold Sleep Research',
    summary: 'Slow-wave and REM sleep cycles replay and consolidate newly formed hippocampal memories into cortical storage.',
  },
  {
    id: 'aerobic-movement',
    label: 'Aerobic movement: Completed physical exercise to boost neuroplasticity and brain blood flow',
    category: 'health',
    evidenceSource: 'Mayo Clinic Brain Health & BDNF Research',
    summary: 'Aerobic exercise releases brain-derived neurotrophic factor (BDNF), stimulating synaptogenesis and dentate gyrus neurogenesis.',
  },
  {
    id: 'social-teaching',
    label: 'Social / teaching engagement: Discussed or taught a concept to someone else',
    category: 'metacognition',
    evidenceSource: 'Protégé Effect & Harvard Summer School Principles',
    summary: 'Articulating concepts to a peer forces coherent structure, semantic precision, and immediate gap detection.',
  },
] as const;
