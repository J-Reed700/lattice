import { useEffect, useState } from 'react';

import {
  Activity,
  BookOpen,
  Brain,
  CheckCircle2,
  Clock,
  ExternalLink,
  Flame,
  GraduationCap,
  Heart,
  Moon,
  RotateCcw,
  Sparkles,
} from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { cn } from '@/lib/utils';

import { CHECKLIST_ITEMS } from './types';
import { useHabitChecklist } from './useHabitChecklist';

import type { FrameworkAction, RetentionFramework } from './types';

export const RETENTION_FRAMEWORKS: readonly RetentionFramework[] = [
  {
    id: 'recall-dump',
    title: 'Active Recall (Testing Yourself)',
    badge: '~80% Retention',
    retentionStat: '~80% retention vs. 34% for passive rereading',
    description: 'Close notes and write/speak everything you remember from scratch (~80% retention vs. 34% for passive rereading).',
    actionLabel: 'Open Free Recall (Brain Dump)',
    keyPrinciples: [
      'Forces cognitive retrieval from long-term memory rather than passive recognition',
      'Exposes illusory mastery and immediately surfaces real knowledge gaps',
      'Strengthens synaptic retrieval pathways under constructive cognitive strain',
    ],
    cognitiveBenefit: 'Synaptic reinforcement via active memory reconstruction',
  },
  {
    id: 'spaced-repetition',
    title: 'Spaced Repetition',
    badge: '48h Curve Shield',
    retentionStat: 'Prevents 70%+ memory decay across expanding intervals',
    description: 'Combat the 48-hour forgetting curve with expanding intervals (1 day, 3 days, 1 week, 2 weeks).',
    actionLabel: 'Open Spaced Repetition & Curve',
    keyPrinciples: [
      'Overcomes Ebbinghaus forgetting curve with systematically expanding intervals',
      'Interval progression (1d, 3d, 1w, 2w) resets memory trace decay at each review',
      'Saves up to 60% of total study time compared to massed single-session cramming',
    ],
    cognitiveBenefit: 'Optimizes synaptic consolidation before memory traces fade',
  },
  {
    id: 'feynman',
    title: 'The Feynman Technique / Teaching Others',
    badge: 'Jargon Detector',
    retentionStat: 'Surfaces 100% of hidden explanatory gaps and illusions',
    description: 'Explain concepts in simple, everyday language as if teaching a beginner; spot jargon and gaps.',
    actionLabel: 'Open Feynman Workbench',
    keyPrinciples: [
      'Explain complex ideas without jargon, buzzwords, or hand-waving',
      'Target specific friction points where intuitive explanations stall',
      'Refine, simplify, and anchor with relatable concrete analogies',
    ],
    cognitiveBenefit: 'Metacognitive calibration and deep schema synthesis',
  },
  {
    id: 'interleaving',
    title: 'Interleaving',
    badge: 'Concept Differentiation',
    retentionStat: 'Up to 43% boost in delayed problem-solving test accuracy',
    description: 'Mix different sub-topics, decks, or problem types in a single session to force concept differentiation.',
    actionLabel: 'Open Interleaved Practice',
    keyPrinciples: [
      'Alternates varied problem types rather than blocked repetitive sets (AAA BBB CCC)',
      'Forces cognitive discrimination: deciding WHICH formula or rule applies',
      'Prevents mindless autopilot habituation during practice sessions',
    ],
    cognitiveBenefit: 'Contextual discrimination and cross-domain pattern matching',
  },
  {
    id: 'dual-coding',
    title: 'Dual Coding & Visualization',
    badge: 'Visual + Verbal Channels',
    retentionStat: 'Double memory pathways for 2x associative retrieval hooks',
    description: 'Pair dense text or abstract numbers with diagrams, Mermaid charts, and visual mental imagery.',
    actionLabel: 'Open Dual-Coding Canvas',
    keyPrinciples: [
      'Engages Paivio\'s dual coding theory: separate verbal and visual cognitive channels',
      'Transforms abstract narrative logic into flowcharts, hierarchy maps, and diagrams',
      'Reduces cognitive load when mastering multi-step architectural relationships',
    ],
    cognitiveBenefit: 'Bi-directional retrieval hooks via visual and linguistic nodes',
  },
  {
    id: 'handwritten-notes',
    title: 'Handwritten Notes & Own-Words Synthesis',
    badge: 'Generative Processing',
    retentionStat: 'Significantly deeper conceptual processing vs. verbatim typing',
    description: 'Write/sketch key takeaways by hand and rephrase ideas rather than transcribing verbatim.',
    actionLabel: 'Open Handwritten & Synthesis Notebook',
    keyPrinciples: [
      'Mueller & Oppenheimer research: manual handwriting forces conceptual reframing',
      'Strictly forbids verbatim transcription: requires real-time synthesis and distillation',
      'Integrates spatial annotations, arrows, callouts, and margin diagrams',
    ],
    cognitiveBenefit: 'Motor-cortex cognitive grounding and own-words synthesis',
  },
] as const;

export interface RetentionFrameworksModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onNavigate?: (action: FrameworkAction) => void;
  onSelectFramework?: (action: FrameworkAction) => void;
  initialTab?: 'frameworks' | 'habits';
}

export function RetentionFrameworksModal({
  open,
  onOpenChange,
  onNavigate,
  onSelectFramework,
  initialTab = 'frameworks',
}: RetentionFrameworksModalProps) {
  const [selectedTab, setSelectedTab] = useState<'frameworks' | 'habits'>(initialTab);
  const { checklist, completedCount, resetAll, toggleItem, totalCount } = useHabitChecklist();

  useEffect(() => {
    if (open) {
      setSelectedTab(initialTab);
    }
  }, [open, initialTab]);

  const handleAction = (action: FrameworkAction) => {
    onSelectFramework?.(action);
    onNavigate?.(action);
    onOpenChange(false);
  };

  const progressPercentage = Math.round((completedCount / (totalCount || 1)) * 100);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[88vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <GraduationCap className="size-6 text-accent" />
            <DialogTitle className="text-xl font-serif text-text-primary">
              Evidence-Based Learning & Retention Guide
            </DialogTitle>
          </div>
          <DialogDescription className="text-sm text-text-muted">
            Cognitive science strategies and physiological habits synthesized from Harvard Summer School retention principles, Mayo Clinic neuroplasticity guidelines, and cognitive load theory.
          </DialogDescription>
        </DialogHeader>

        <Tabs value={selectedTab} onValueChange={(val) => setSelectedTab(val as 'frameworks' | 'habits')} className="w-full mt-2">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="frameworks">
              <Brain className="size-4 mr-1.5" />
              Core Retention Frameworks
            </TabsTrigger>
            <TabsTrigger value="habits">
              <Heart className="size-4 mr-1.5" />
              Harvard Principles & Health ({completedCount}/{totalCount})
            </TabsTrigger>
          </TabsList>

          <TabsContent value="frameworks" className="space-y-4 pt-3">
            <div className="rounded-lg border border-border-subtle bg-surface-raised p-3.5 text-xs text-text-secondary flex items-start gap-3">
              <Sparkles className="size-5 shrink-0 text-accent mt-0.5" />
              <div>
                <p className="font-medium text-text-primary">
                  Active Retrieval Outperforms Passive Review by ~2.4x
                </p>
                <p className="mt-0.5 text-text-secondary">
                  Studies confirm that passive rereading results in only ~34% retention after 48 hours, whereas active retrieval practice locks in ~80%. Choose a learning framework below to jump directly into deliberate practice.
                </p>
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {RETENTION_FRAMEWORKS.map((framework) => (
                <Card key={framework.id} className="flex flex-col justify-between border-border-subtle bg-surface hover:border-border-default transition-colors">
                  <CardHeader className="p-4 pb-2">
                    <div className="flex items-start justify-between gap-2">
                      <CardTitle className="text-base font-semibold leading-snug text-text-primary">
                        {framework.title}
                      </CardTitle>
                      <Badge variant="secondary" className="shrink-0 text-[10px] bg-surface-raised text-text-secondary border-border-subtle">
                        {framework.badge}
                      </Badge>
                    </div>
                    <CardDescription className="text-xs mt-1.5 leading-relaxed text-text-muted">
                      {framework.description}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="p-4 pt-2 flex flex-col justify-between flex-1 gap-3">
                    <div className="space-y-1.5">
                      <div className="rounded bg-surface-sunken px-2.5 py-1 text-[11px] font-medium text-text-secondary border border-border-subtle/50">
                        Evidence: {framework.retentionStat}
                      </div>
                      <ul className="space-y-1 text-xs text-text-muted list-disc pl-4 pt-1">
                        {framework.keyPrinciples.map((principle) => (
                          <li key={principle} className="leading-snug">{principle}</li>
                        ))}
                      </ul>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      className="w-full mt-2 text-xs font-medium justify-center border-border-subtle hover:border-border-default hover:bg-surface-raised text-text-primary"
                      onClick={() => handleAction(framework.id)}
                    >
                      {framework.actionLabel}
                    </Button>
                  </CardContent>
                </Card>
              ))}
            </div>
          </TabsContent>

          <TabsContent value="habits" className="space-y-5 pt-3">
            {/* Metacognitive Checklist Section */}
            <div className="rounded-lg border border-border-subtle bg-surface p-4 space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-sm font-semibold text-text-primary flex items-center gap-1.5">
                    <CheckCircle2 className="size-4 text-accent" />
                    Metacognitive Self-Assessment & Daily Habits Checklist
                  </h3>
                  <p className="text-xs text-text-muted mt-0.5">
                    Harvard retention principles emphasize verifying understanding and maintaining physiological cognitive readiness.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-accent">
                    {completedCount} of {totalCount} completed ({progressPercentage}%)
                  </span>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-text-muted hover:text-text-primary hover:bg-surface-raised"
                    onClick={resetAll}
                    title="Reset all checklist items"
                  >
                    <RotateCcw className="size-3 mr-1" />
                    Reset
                  </Button>
                </div>
              </div>

              {/* Progress Bar */}
              <div className="h-1.5 w-full rounded-full bg-surface-sunken overflow-hidden">
                <div
                  className="h-full bg-accent transition-all duration-300"
                  style={{ width: `${progressPercentage}%` }}
                />
              </div>

              {/* Checklist items */}
              <div className="space-y-2 pt-1">
                {CHECKLIST_ITEMS.map((item) => {
                  const isChecked = Boolean(checklist[item.id]);
                  return (
                    <label
                      key={item.id}
                      className="flex items-start gap-3 p-2.5 rounded-md border border-border-subtle bg-surface-raised hover:bg-surface-overlay cursor-pointer transition-colors"
                    >
                      <input
                        type="checkbox"
                        checked={isChecked}
                        onChange={() => toggleItem(item.id)}
                        className="mt-0.5 size-4 rounded border-border-default text-accent focus:ring-accent cursor-pointer"
                      />
                      <div className="flex-1 space-y-0.5">
                        <p className={cn('text-xs font-medium leading-relaxed', isChecked ? 'line-through text-text-muted' : 'text-text-primary')}>
                          {item.label}
                        </p>
                        <p className="text-[11px] text-text-muted">
                          {item.summary} • <span className="italic">{item.evidenceSource}</span>
                        </p>
                      </div>
                    </label>
                  );
                })}
              </div>
            </div>

            {/* Harvard Summer School Retention Principles */}
            <div className="rounded-lg border border-border-subtle bg-surface p-4 space-y-2.5">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold text-text-primary flex items-center gap-1.5">
                  <BookOpen className="size-4 text-accent" />
                  Harvard Summer School Retention Principles
                </h3>
                <a
                  href="https://summer.harvard.edu/blog/5-tips-to-retain-what-you-learn/"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-xs text-accent hover:underline font-medium"
                >
                  Read Harvard Guide
                  <ExternalLink className="size-3" />
                </a>
              </div>
              <p className="text-xs text-text-secondary leading-relaxed">
                Harvard pedagogical research identifies five pivotal behaviors that separate durable mastery from transient cramming:
              </p>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5 pt-1 text-xs text-text-secondary">
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <p className="font-semibold text-text-primary">1. Spacing Out Study Sessions</p>
                  <p className="text-[11px] text-text-muted mt-1">
                    Short, frequent study blocks over days and weeks trigger re-consolidation and reinforce synaptic connections far deeper than all-night cramming.
                  </p>
                </div>
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <p className="font-semibold text-text-primary">2. Frequent Retrieval Practice</p>
                  <p className="text-[11px] text-text-muted mt-1">
                    Regularly testing your memory through flashcards, brain dumps, and practice quizzes proves durable knowledge far more than passively highlighting textbooks.
                  </p>
                </div>
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <p className="font-semibold text-text-primary">3. Metacognitive Calibration</p>
                  <p className="text-[11px] text-text-muted mt-1">
                    Reflect continually on what you truly grasp versus illusory familiarity. Distinguish recognizing a formula from reproducing it autonomously.
                  </p>
                </div>
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <p className="font-semibold text-text-primary">4. Own-Words Synthesis & Teaching</p>
                  <p className="text-[11px] text-text-muted mt-1">
                    Translating terminology into plain english or teaching a peer forces real-time cognitive gap detection and builds coherent schemas.
                  </p>
                </div>
              </div>
            </div>

            {/* Mayo Clinic Physical & Mental Health Section */}
            <div className="rounded-lg border border-border-subtle bg-surface p-4 space-y-2.5">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold text-text-primary flex items-center gap-1.5">
                  <Activity className="size-4 text-accent" />
                  Mayo Clinic Physical & Mental Health Memory Foundations
                </h3>
                <a
                  href="https://www.mayoclinic.org/healthy-lifestyle/healthy-aging/in-depth/memory-loss/art-20046518"
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-xs text-accent hover:underline font-medium"
                >
                  Read Mayo Clinic Guide
                  <ExternalLink className="size-3" />
                </a>
              </div>
              <p className="text-xs text-text-secondary leading-relaxed">
                Cognitive performance is strictly anchored to neurobiology. Mayo Clinic clinical guidelines emphasize these non-negotiable physiological pillars:
              </p>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-2.5 pt-1 text-xs text-text-secondary">
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <div className="flex items-center gap-1 font-semibold text-text-primary">
                    <Moon className="size-3.5 text-blue-500" />
                    Sleep (7–8+ Hours)
                  </div>
                  <p className="text-[11px] text-text-muted mt-1">
                    Slow-wave and REM sleep cycles replay hippocampal memory traces into neocortical long-term storage while clearing metabolic waste.
                  </p>
                </div>
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <div className="flex items-center gap-1 font-semibold text-text-primary">
                    <Activity className="size-3.5 text-emerald-500" />
                    Aerobic Movement & BDNF
                  </div>
                  <p className="text-[11px] text-text-muted mt-1">
                    Daily moderate aerobic activity increases brain-derived neurotrophic factor (BDNF), stimulating adult neurogenesis and cerebral circulation.
                  </p>
                </div>
                <div className="rounded border border-border-subtle p-2.5 bg-surface-raised">
                  <div className="flex items-center gap-1 font-semibold text-text-primary">
                    <Flame className="size-3.5 text-amber-500" />
                    Anti-Cramming Rest
                  </div>
                  <p className="text-[11px] text-text-muted mt-1">
                    Continuous study past 90 minutes induces cognitive fatigue and elevates cortisol, degrading synaptic plasticity. Schedule regular 5-minute pauses.
                  </p>
                </div>
              </div>
            </div>

            {/* Pomodoro & Single-Tasking Principles */}
            <div className="rounded-lg border border-border-subtle bg-surface p-4 flex items-start gap-3">
              <Clock className="size-5 shrink-0 text-accent mt-0.5" />
              <div className="text-xs text-text-secondary space-y-1">
                <p className="font-semibold text-text-primary">
                  Pomodoro 30–45 Minute Focused Intervals & Single-Tasking Guard
                </p>
                <p className="leading-relaxed">
                  Working memory can only sustain high-fidelity attention for 30–45 minutes before interference builds up. Rapid context-switching or studying while checking devices causes cognitive thrashing and degrades retention by over 40%. Use the top Pomodoro bar to enforce single-tasking intervals.
                </p>
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
