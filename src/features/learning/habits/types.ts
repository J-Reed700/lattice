export type FrameworkAction =
  | 'recall-dump'
  | 'spaced-repetition'
  | 'feynman'
  | 'interleaving'
  | 'dual-coding'
  | 'handwritten-notes';

/**
 * How strong the research behind a technique is, in the terms of the
 * learning science lessons design (§2): strong for findings replicated
 * across many studies and settings, moderate for consistent benefits with
 * known limits, mixed where findings conflict or a popular claim failed to
 * replicate.
 */
export type EvidenceStrength = 'strong' | 'moderate' | 'mixed';

/** A published source, linked by its DOI. */
export interface Citation {
  label: string;
  doi: string;
}

export interface RetentionFramework {
  id: FrameworkAction;
  title: string;
  strength: EvidenceStrength;
  /** What to do. */
  description: string;
  /** What research found, including its limits. */
  evidence: string;
  citations: readonly Citation[];
  /** Where Lattice's lessons already do it for you. */
  inLattice: string;
  actionLabel: string;
}

export interface ChecklistItem {
  id: string;
  label: string;
  category: 'cognition' | 'health' | 'metacognition';
  summary: string;
  citations: readonly Citation[];
}

export type HabitChecklistState = Record<string, boolean>;

export const HABIT_CHECKLIST_STORAGE_KEY = 'lattice.learning.habits.checklist';

/** The sources the guide cites; every DOI was checked against Crossref. */
export const SOURCES = {
  dunlosky2013: { label: 'Dunlosky et al., 2013', doi: '10.1177/1529100612453266' },
  roediger2006: { label: 'Roediger & Karpicke, 2006', doi: '10.1111/j.1467-9280.2006.01693.x' },
  adesope2017: { label: 'Adesope, Trevisan & Sundararajan, 2017', doi: '10.3102/0034654316689306' },
  cepeda2006: { label: 'Cepeda et al., 2006', doi: '10.1037/0033-2909.132.3.354' },
  cepeda2008: { label: 'Cepeda et al., 2008', doi: '10.1111/j.1467-9280.2008.02209.x' },
  karpicke2007: { label: 'Karpicke & Roediger, 2007', doi: '10.1037/0278-7393.33.4.704' },
  bisra2018: { label: 'Bisra et al., 2018', doi: '10.1007/s10648-018-9434-x' },
  fiorella2013: { label: 'Fiorella & Mayer, 2013', doi: '10.1016/j.cedpsych.2013.06.001' },
  fiorella2016: { label: 'Fiorella & Mayer, 2016', doi: '10.1007/s10648-015-9348-9' },
  rohrer2007: { label: 'Rohrer & Taylor, 2007', doi: '10.1007/s11251-007-9015-8' },
  brunmair2019: { label: 'Brunmair & Richter, 2019', doi: '10.1037/bul0000209' },
  alfieri2013: { label: 'Alfieri, Nokes-Malach & Schunn, 2013', doi: '10.1080/00461520.2013.775712' },
  clark1991: { label: 'Clark & Paivio, 1991', doi: '10.1007/BF01320076' },
  weinstein2018: { label: 'Weinstein, Madan & Sumeracki, 2018', doi: '10.1186/s41235-017-0087-y' },
  morehead2019: { label: 'Morehead, Dunlosky & Rawson, 2019', doi: '10.1007/s10648-019-09468-2' },
  biwer2023: { label: 'Biwer et al., 2023', doi: '10.1111/bjep.12593' },
  richland2009: { label: 'Richland, Kornell & Kao, 2009', doi: '10.1037/a0016496' },
  pan2023: { label: 'Pan & Carpenter, 2023', doi: '10.1007/s10648-023-09814-5' },
  rasch2013: { label: 'Rasch & Born, 2013', doi: '10.1152/physrev.00032.2012' },
  roig2013: { label: 'Roig et al., 2013', doi: '10.1016/j.neubiorev.2013.06.012' },
  pashler2008: { label: 'Pashler et al., 2008', doi: '10.1111/j.1539-6053.2009.01038.x' },
} as const satisfies Record<string, Citation>;

export const CHECKLIST_ITEMS: readonly ChecklistItem[] = [
  {
    id: 'pre-study-recall',
    label: 'Before reading, tried to recall what I already know, or guessed the answers',
    category: 'metacognition',
    summary: 'Attempting questions before studying improves learning of that material, even when the guesses are wrong.',
    citations: [SOURCES.richland2009, SOURCES.pan2023],
  },
  {
    id: 'own-words-synthesis',
    label: 'Explained the key ideas in my own words without looking',
    category: 'cognition',
    summary: 'Recalling and explaining helps more than rereading or copying.',
    citations: [SOURCES.fiorella2016, SOURCES.roediger2006],
  },
  {
    id: 'social-teaching',
    label: 'Explained a concept to someone else',
    category: 'metacognition',
    summary: 'Explaining to others helps most when you have to recall the ideas rather than read out notes.',
    citations: [SOURCES.fiorella2013],
  },
  {
    id: 'focused-block',
    label: 'Studied in a focused block, without switching to messages or other tasks',
    category: 'cognition',
    summary: 'Fixed work-and-break intervals such as Pomodoro mainly help with fatigue and mood; no particular interval length is shown to improve learning. Pick one that keeps you focused.',
    citations: [SOURCES.biwer2023],
  },
  {
    id: 'sleep-consolidation',
    label: 'Slept well after studying',
    category: 'health',
    summary: 'Sleep after learning supports the consolidation of new memories.',
    citations: [SOURCES.rasch2013],
  },
  {
    id: 'aerobic-movement',
    label: 'Did some aerobic exercise',
    category: 'health',
    summary: 'Studies find aerobic exercise can benefit memory, with effects that vary by timing and person. It supports study; it does not replace it.',
    citations: [SOURCES.roig2013],
  },
] as const;
