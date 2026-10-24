import { useMemo } from 'react';

import { Layers, Sparkles, TrendingUp } from 'lucide-react';

import { cn } from '@/lib/utils';

export interface SpacedRepetitionCard {
  id: string;
  dueAt: number;
  intervalDays: number;
  reviewCount: number;
  stability?: number | null;
}

export interface SpacedRepetitionCurveProps {
  cards: SpacedRepetitionCard[];
}

interface TierDefinition {
  id: string;
  label: string;
  rangeDescription: string;
  filter: (card: SpacedRepetitionCard) => boolean;
  colorClass: string;
  bgClass: string;
}

const INTERVAL_TIERS: TierDefinition[] = [
  {
    id: 'initial',
    label: 'Initial (< 1 day)',
    rangeDescription: 'New or struggling cards',
    filter: (c) => c.intervalDays < 1,
    colorClass: 'text-text-muted',
    bgClass: 'bg-surface-raised',
  },
  {
    id: 'tier-1-2',
    label: '1–2 days',
    rangeDescription: 'First critical recall test',
    filter: (c) => c.intervalDays >= 1 && c.intervalDays < 3,
    colorClass: 'text-warning-fg',
    bgClass: 'bg-warning-muted/40',
  },
  {
    id: 'tier-3-6',
    label: '3–6 days',
    rangeDescription: 'Consolidating retention',
    filter: (c) => c.intervalDays >= 3 && c.intervalDays < 7,
    colorClass: 'text-accent',
    bgClass: 'bg-accent/20',
  },
  {
    id: 'tier-7-13',
    label: '1 week (7–13 days)',
    rangeDescription: 'Medium-term durability',
    filter: (c) => c.intervalDays >= 7 && c.intervalDays < 14,
    colorClass: 'text-success-fg',
    bgClass: 'bg-success-muted/40',
  },
  {
    id: 'tier-14-plus',
    label: '2+ weeks (14+ days)',
    rangeDescription: 'Long-term mastery',
    filter: (c) => c.intervalDays >= 14,
    colorClass: 'text-accent',
    bgClass: 'bg-accent/40',
  },
];

export function SpacedRepetitionCurve({ cards }: SpacedRepetitionCurveProps) {
  const tierDistribution = useMemo(() => {
    const total = cards.length;
    return INTERVAL_TIERS.map((tier) => {
      const matching = cards.filter(tier.filter);
      const count = matching.length;
      const percentage = total > 0 ? Math.round((count / total) * 100) : 0;
      return {
        ...tier,
        count,
        percentage,
      };
    });
  }, [cards]);

  const matureCardsCount = useMemo(
    () => cards.filter((c) => c.intervalDays >= 14).length,
    [cards],
  );

  return (
    <section
      aria-label="Spaced Repetition & The Forgetting Curve"
      className="rounded-2xl border border-border-default bg-surface p-4 shadow-xs"
    >
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-2.5">
        <div>
          <div className="inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[.14em] text-accent">
            <TrendingUp size={13} />
            <span>Spaced Repetition · Expanding Intervals</span>
          </div>
          <h3 className="mt-0.5 font-serif text-lg font-medium text-text-primary">
            The Ebbinghaus Forgetting Curve &amp; Memory Reset
          </h3>
        </div>

        <div className="flex items-center gap-2">
          <span className="inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface-sunken px-2.5 py-0.5 text-xs font-medium text-text-secondary">
            <Layers size={12} className="text-accent" />
            <span>{cards.length} scheduled cards</span>
          </span>
          {cards.length > 0 && (
            <span className="inline-flex items-center gap-1 rounded-full border border-success-muted bg-success-muted/30 px-2.5 py-0.5 text-xs font-medium text-success-fg">
              <Sparkles size={12} />
              <span>{matureCardsCount} mastered (14d+)</span>
            </span>
          )}
        </div>
      </div>

      <p className="mt-1 text-xs leading-relaxed text-text-secondary">
        Combat the forgetting curve (where most new information is lost within 48 hours) by reviewing material at gradually expanding intervals: 1 day → 3 days → 1 week → 2 weeks+.
      </p>

      {/* Main Grid: Ebbinghaus Curve (left) + Tier Bars (right) */}
      <div className="mt-3.5 grid gap-3.5 items-start lg:grid-cols-[1.2fr_1fr]">
        {/* SVG Forgetting Curve & Spaced Repetition Chart */}
        <div className="rounded-xl border border-border-default bg-surface-sunken/40 p-3">
          <div className="flex flex-wrap items-center justify-between gap-1.5 pb-2">
            <span className="text-xs font-medium text-text-primary">
              Retention vs. Time (Days)
            </span>
            <div className="flex flex-wrap items-center gap-3 text-[10px]">
              <span className="inline-flex items-center gap-1.5 text-danger-fg">
                <span className="inline-block h-0.5 w-3 border-b-2 border-dashed border-danger-fg" />
                <span>Unreviewed (drops in 48h)</span>
              </span>
              <span className="inline-flex items-center gap-1.5 font-medium text-accent">
                <span className="inline-block h-0.5 w-3 bg-accent" />
                <span>Expanding Spaced Reviews</span>
              </span>
            </div>
          </div>

          <div className="overflow-x-auto">
            <svg
              viewBox="0 0 600 220"
              className="h-36 w-full min-w-[380px] select-none"
              aria-label="Ebbinghaus Forgetting Curve and Spaced Repetition Intervals graph"
              role="img"
            >
              {/* Background grid lines */}
              <line x1="45" y1="30" x2="570" y2="30" stroke="currentColor" strokeOpacity="0.08" />
              <line x1="45" y1="75" x2="570" y2="75" stroke="currentColor" strokeOpacity="0.08" />
              <line x1="45" y1="120" x2="570" y2="120" stroke="currentColor" strokeOpacity="0.08" />
              <line x1="45" y1="165" x2="570" y2="165" stroke="currentColor" strokeOpacity="0.08" />

              {/* Y Axis labels */}
              <text x="38" y="34" textAnchor="end" className="fill-text-muted text-[10px]">100%</text>
              <text x="38" y="79" textAnchor="end" className="fill-text-muted text-[10px]">75%</text>
              <text x="38" y="124" textAnchor="end" className="fill-text-muted text-[10px]">50%</text>
              <text x="38" y="169" textAnchor="end" className="fill-text-muted text-[10px]">25%</text>
              <text x="38" y="195" textAnchor="end" className="fill-text-muted text-[10px]">0%</text>

              {/* X Axis line */}
              <line x1="45" y1="190" x2="570" y2="190" stroke="currentColor" strokeOpacity="0.2" strokeWidth="1" />

              {/* X Axis ticks & labels */}
              <text x="55" y="206" textAnchor="middle" className="fill-text-muted text-[10px]">Day 0</text>
              <line x1="125" y1="30" x2="125" y2="190" stroke="currentColor" strokeOpacity="0.06" strokeDasharray="2 2" />
              <text x="125" y="206" textAnchor="middle" className="fill-text-secondary font-medium text-[10px]">Day 1</text>
              <line x1="210" y1="30" x2="210" y2="190" stroke="currentColor" strokeOpacity="0.06" strokeDasharray="2 2" />
              <text x="210" y="206" textAnchor="middle" className="fill-text-secondary font-medium text-[10px]">Day 3</text>
              <line x1="320" y1="30" x2="320" y2="190" stroke="currentColor" strokeOpacity="0.06" strokeDasharray="2 2" />
              <text x="320" y="206" textAnchor="middle" className="fill-text-secondary font-medium text-[10px]">Day 7 (1 wk)</text>
              <line x1="440" y1="30" x2="440" y2="190" stroke="currentColor" strokeOpacity="0.06" strokeDasharray="2 2" />
              <text x="440" y="206" textAnchor="middle" className="fill-text-secondary font-medium text-[10px]">Day 14 (2 wks)</text>
              <line x1="560" y1="30" x2="560" y2="190" stroke="currentColor" strokeOpacity="0.06" strokeDasharray="2 2" />
              <text x="560" y="206" textAnchor="middle" className="fill-text-secondary font-medium text-[10px]">Day 30 (1 mo)</text>

              {/* Curve 1: Unreviewed rapid forgetting curve (Dashed Red/Amber) */}
              <path
                d="M 55 30 Q 80 140, 125 155 T 210 176 T 320 182 T 560 186"
                fill="none"
                stroke="var(--color-danger-fg, #ef4444)"
                strokeWidth="2"
                strokeDasharray="4 4"
              />
              <circle cx="165" cy="168" r="3" fill="var(--color-danger-fg, #ef4444)" />
              <text x="170" y="162" className="fill-danger-fg text-[9px] font-semibold">48h drop (&lt;30%)</text>

              {/* Curve 2: Expanding Spaced Repetition reset curves (Solid Accent) */}
              <path
                d="M 55 30 Q 90 70, 125 95"
                fill="none"
                stroke="var(--color-accent, #6366f1)"
                strokeWidth="2.5"
              />
              <line x1="125" y1="95" x2="125" y2="30" stroke="var(--color-accent, #6366f1)" strokeWidth="1.5" strokeDasharray="2 2" />
              <circle cx="125" cy="30" r="3.5" fill="var(--color-accent, #6366f1)" />

              <path
                d="M 125 30 Q 170 65, 210 80"
                fill="none"
                stroke="var(--color-accent, #6366f1)"
                strokeWidth="2.5"
              />
              <line x1="210" y1="80" x2="210" y2="30" stroke="var(--color-accent, #6366f1)" strokeWidth="1.5" strokeDasharray="2 2" />
              <circle cx="210" cy="30" r="3.5" fill="var(--color-accent, #6366f1)" />

              <path
                d="M 210 30 Q 270 55, 320 68"
                fill="none"
                stroke="var(--color-accent, #6366f1)"
                strokeWidth="2.5"
              />
              <line x1="320" y1="68" x2="320" y2="30" stroke="var(--color-accent, #6366f1)" strokeWidth="1.5" strokeDasharray="2 2" />
              <circle cx="320" cy="30" r="3.5" fill="var(--color-accent, #6366f1)" />

              <path
                d="M 320 30 Q 380 48, 440 58"
                fill="none"
                stroke="var(--color-accent, #6366f1)"
                strokeWidth="2.5"
              />
              <line x1="440" y1="58" x2="440" y2="30" stroke="var(--color-accent, #6366f1)" strokeWidth="1.5" strokeDasharray="2 2" />
              <circle cx="440" cy="30" r="3.5" fill="var(--color-accent, #6366f1)" />

              <path
                d="M 440 30 Q 500 42, 560 50"
                fill="none"
                stroke="var(--color-accent, #6366f1)"
                strokeWidth="2.5"
              />
              <circle cx="560" cy="50" r="3.5" fill="var(--color-accent, #6366f1)" />

              {/* Interval annotations */}
              <text x="125" y="24" textAnchor="middle" className="fill-accent text-[9px] font-semibold">+1d</text>
              <text x="210" y="24" textAnchor="middle" className="fill-accent text-[9px] font-semibold">+3d</text>
              <text x="320" y="24" textAnchor="middle" className="fill-accent text-[9px] font-semibold">+1wk</text>
              <text x="440" y="24" textAnchor="middle" className="fill-accent text-[9px] font-semibold">+2wks</text>
              <text x="560" y="24" textAnchor="middle" className="fill-accent text-[9px] font-semibold">+1mo</text>
            </svg>
          </div>
        </div>

        {/* Live Learner Card Distribution Across Expanding Interval Tiers */}
        <div className="flex flex-col justify-between rounded-xl border border-border-default bg-surface-sunken/40 p-3">
          <div className="flex items-center justify-between border-b border-border-subtle pb-2">
            <h4 className="font-serif text-xs font-semibold text-text-primary">
              Your Cards Across Interval Tiers
            </h4>
            <span className="text-[11px] text-text-muted">
              {cards.length} total card{cards.length === 1 ? '' : 's'}
            </span>
          </div>

          <div className="mt-2.5 flex flex-col gap-2">
            {tierDistribution.map((tier) => (
              <div
                key={tier.id}
                className="rounded-lg border border-border-subtle/80 bg-surface px-2.5 py-1.5"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-baseline gap-1.5 truncate">
                    <span className="truncate text-xs font-semibold text-text-primary">
                      {tier.label}
                    </span>
                    <span className="hidden text-[10px] text-text-muted sm:inline">
                      · {tier.rangeDescription}
                    </span>
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    <span className={cn('text-xs font-bold', tier.colorClass)}>
                      {tier.count}
                    </span>
                    <span className="text-[10px] text-text-muted">
                      ({tier.percentage}%)
                    </span>
                  </div>
                </div>

                <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-sunken">
                  <div
                    className={cn('h-full transition-all duration-300', tier.bgClass)}
                    style={{ width: `${tier.percentage}%` }}
                  />
                </div>
              </div>
            ))}
          </div>

          {cards.length === 0 && (
            <p className="mt-2 text-center text-xs text-text-muted">
              No recall cards currently scheduled. Author cards or generate decks to begin expanding your intervals.
            </p>
          )}
        </div>
      </div>
    </section>
  );
}
