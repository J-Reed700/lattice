import { useEffect, useState } from 'react';

import { Brain, CheckCircle2, CircleSlash, GraduationCap, Heart, RotateCcw, Sparkles } from 'lucide-react';

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

import { CHECKLIST_ITEMS, SOURCES } from './types';
import { useHabitChecklist } from './useHabitChecklist';

import type { Citation, EvidenceStrength, FrameworkAction, RetentionFramework } from './types';

/**
 * The techniques, as the learning science lessons design (§2) weighs them:
 * what to do, what research found and its limits, and where the lessons
 * already build it in. No percentages: a retention figure from one study is
 * not a promise about anyone's learning.
 */
export const RETENTION_FRAMEWORKS: readonly RetentionFramework[] = [
  {
    id: 'recall-dump',
    title: 'Retrieval practice',
    strength: 'strong',
    description: 'Close the material and bring it back from memory: write it out, answer questions, or explain it aloud. A wrong attempt followed by the answer still helps.',
    evidence: 'One of the most replicated findings in learning research: practising retrieval leads to better long-term retention than rereading the same material, in laboratories and in classrooms. The benefit is largest after a delay, and when feedback follows.',
    citations: [SOURCES.roediger2006, SOURCES.adesope2017, SOURCES.dunlosky2013],
    inLattice: 'Every lesson asks you to commit to an answer before its reasoning is revealed, and to explain the lesson from memory before you see the key takeaways.',
    actionLabel: 'Open free recall',
  },
  {
    id: 'spaced-repetition',
    title: 'Spaced practice',
    strength: 'strong',
    description: 'Return to material over days and weeks instead of all in one sitting.',
    evidence: 'Spreading the same study time across sessions reliably beats massing it. The best gap grows with how long you need to remember; expanding and equal gaps both work, and what matters most is that each return is a successful recall.',
    citations: [SOURCES.cepeda2006, SOURCES.cepeda2008, SOURCES.karpicke2007],
    inLattice: 'Review cards are scheduled by FSRS, which sets each card’s next review from how you answered. A lesson’s checks join your reviews when you answer them, and warm-ups bring due ideas back before new lessons.',
    actionLabel: 'Open your review schedule',
  },
  {
    id: 'feynman',
    title: 'Explain it simply',
    strength: 'moderate',
    description: 'Explain the idea in plain words, as if to someone new to it. Where your explanation stalls is what you do not yet understand.',
    evidence: 'Prompting learners to explain material to themselves improves understanding, with a moderate average benefit across studies. Explaining to others helps most when it means recalling the ideas rather than reading from notes.',
    citations: [SOURCES.bisra2018, SOURCES.fiorella2013],
    inLattice: 'Each lesson ends by asking you to explain it from memory, as if teaching it, and then compare with the key takeaways.',
    actionLabel: 'Open the explanation workbench',
  },
  {
    id: 'interleaving',
    title: 'Interleaving and contrasting cases',
    strength: 'moderate',
    description: 'Mix related kinds of problems, and study an idea beside a near-miss that looks similar but works differently.',
    evidence: 'Mixing problem types helps you learn to choose the right approach, most of all when the kinds are easy to confuse; with very different material it helps less, or not at all. Comparing cases side by side improves learning and transfer.',
    citations: [SOURCES.rohrer2007, SOURCES.brunmair2019, SOURCES.alfieri2013],
    inLattice: 'Lessons set an idea beside a deceptively similar case and name the one feature that changes the outcome; later checks ask which idea applies.',
    actionLabel: 'Open mixed review',
  },
  {
    id: 'dual-coding',
    title: 'Words with pictures',
    strength: 'moderate',
    description: 'Pair an explanation with a diagram, table or sketch of its structure: parts and how they relate, a flow, a sequence.',
    evidence: 'People generally learn more from words with relevant pictures than from words alone. Decorative pictures do not help and can distract.',
    citations: [SOURCES.clark1991, SOURCES.weinstein2018],
    inLattice: 'Lessons include a diagram when an idea has structure, and the canvas has templates for drawing your own.',
    actionLabel: 'Open the canvas',
  },
  {
    id: 'handwritten-notes',
    title: 'Notes in your own words',
    strength: 'mixed',
    description: 'Summarise and connect ideas in your own words rather than copying them, by hand or typed.',
    evidence: 'Generating your own summaries, maps and explanations helps learning. The well-known finding that handwritten notes beat laptop notes did not hold up in a larger replication: what matters is working the ideas over, not the pen.',
    citations: [SOURCES.fiorella2016, SOURCES.morehead2019],
    inLattice: 'The notebook keeps your notes beside each lesson.',
    actionLabel: 'Open the notebook',
  },
] as const;

const STRENGTH_LABEL: Record<EvidenceStrength, string> = {
  strong: 'Strong evidence',
  moderate: 'Moderate evidence',
  mixed: 'Mixed evidence',
};

function Citations({ citations }: { citations: readonly Citation[] }) {
  return (
    <span>
      {citations.map((citation, index) => (
        <span key={citation.doi}>
          {index > 0 && '; '}
          <a href={`https://doi.org/${citation.doi}`} target="_blank" rel="noreferrer" className="text-accent hover:underline">{citation.label}</a>
        </span>
      ))}
    </span>
  );
}

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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[88vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <GraduationCap className="size-6 text-accent" />
            <DialogTitle className="text-xl font-serif text-text-primary">
              How to learn so it lasts
            </DialogTitle>
          </div>
          <DialogDescription className="text-sm text-text-muted">
            What learning research supports, how strong the evidence is, and where your lessons already build it in.
          </DialogDescription>
        </DialogHeader>

        <Tabs value={selectedTab} onValueChange={(val) => setSelectedTab(val as 'frameworks' | 'habits')} className="w-full mt-2">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="frameworks">
              <Brain className="size-4 mr-1.5" />
              Techniques
            </TabsTrigger>
            <TabsTrigger value="habits">
              <Heart className="size-4 mr-1.5" />
              Habits ({completedCount}/{totalCount})
            </TabsTrigger>
          </TabsList>

          <TabsContent value="frameworks" className="space-y-4 pt-3">
            <div className="rounded-lg border border-border-subtle bg-surface-raised p-3.5 text-xs text-text-secondary flex items-start gap-3">
              <Sparkles className="size-5 shrink-0 text-accent mt-0.5" />
              <div>
                <p className="font-medium text-text-primary">The two strongest: recall it, and come back to it</p>
                <p className="mt-0.5 text-text-secondary">
                  A major review of ten common techniques rated practice testing and spaced practice most useful, and rereading and highlighting least (<Citations citations={[SOURCES.dunlosky2013]} />). The others below help too, within the limits each one notes.
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
                        {STRENGTH_LABEL[framework.strength]}
                      </Badge>
                    </div>
                    <CardDescription className="text-xs mt-1.5 leading-relaxed text-text-muted">
                      {framework.description}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="p-4 pt-2 flex flex-col justify-between flex-1 gap-3">
                    <div className="space-y-2 text-xs leading-relaxed">
                      <p className="text-text-secondary">
                        <span className="font-medium text-text-primary">What research found: </span>
                        {framework.evidence} <Citations citations={framework.citations} />
                      </p>
                      <p className="text-text-secondary">
                        <span className="font-medium text-text-primary">In your lessons: </span>
                        {framework.inLattice}
                      </p>
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
            <div className="rounded-lg border border-border-subtle bg-surface p-4 space-y-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h3 className="text-sm font-semibold text-text-primary flex items-center gap-1.5">
                    <CheckCircle2 className="size-4 text-accent" />
                    Today’s study habits
                  </h3>
                  <p className="text-xs text-text-muted mt-0.5">
                    A private checklist on this device. It records what you did, not how much you learned.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold text-accent">
                    {completedCount} of {totalCount}
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
                          {item.summary} <Citations citations={item.citations} />
                        </p>
                      </div>
                    </label>
                  );
                })}
              </div>
            </div>

            <div className="rounded-lg border border-border-subtle bg-surface p-4 space-y-2">
              <h3 className="text-sm font-semibold text-text-primary flex items-center gap-1.5">
                <CircleSlash className="size-4 text-accent" />
                What the evidence does not support
              </h3>
              <ul className="list-disc space-y-1.5 pl-5 text-xs leading-relaxed text-text-secondary">
                <li>
                  Matching lessons to a “learning style” (visual, auditory and so on): there is no good evidence that it improves learning (<Citations citations={[SOURCES.pashler2008]} />).
                </li>
                <li>
                  Rereading and highlighting as main strategies: they feel productive but help little compared with recalling (<Citations citations={[SOURCES.dunlosky2013]} />).
                </li>
                <li>
                  Handwriting notes as a rule: what helps is putting ideas in your own words, by hand or typed (<Citations citations={[SOURCES.morehead2019]} />).
                </li>
              </ul>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
