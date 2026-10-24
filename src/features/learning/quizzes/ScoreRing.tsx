import type { Tone } from '@/features/learning/quizzes/quizCopy';

const SIZE = 132;
const STROKE = 10;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

const ringColour: Partial<Record<Tone, string>> = {
  passed: 'stroke-emerald-600 dark:stroke-emerald-400',
  notPassed: 'stroke-accent',
  pending: 'stroke-accent/45',
  neutral: 'stroke-accent',
};

/**
 * The score as a ring with a tick at the pass mark. Decorative: the same
 * numbers are always written beside it, so it is hidden from assistive tech.
 */
export function ScoreRing({ score, passMark, tone, label }: { score: number; passMark: number | null; tone: Tone; label: string }) {
  const clamped = Math.max(0, Math.min(1, score));
  const tick = passMark === null ? null : (passMark * 360 - 90) * (Math.PI / 180);
  const centre = SIZE / 2;
  return <div aria-hidden="true" className="relative grid shrink-0 place-items-center" style={{ width: SIZE, height: SIZE }}>
    <svg width={SIZE} height={SIZE} viewBox={`0 0 ${SIZE} ${SIZE}`} className="-rotate-90">
      <circle cx={centre} cy={centre} r={RADIUS} fill="none" strokeWidth={STROKE} className="stroke-black/8 dark:stroke-white/10" />
      <circle cx={centre} cy={centre} r={RADIUS} fill="none" strokeWidth={STROKE} strokeLinecap="round"
        strokeDasharray={`${clamped * CIRCUMFERENCE} ${CIRCUMFERENCE}`} strokeDashoffset={0}
        className={`${ringColour[tone] ?? 'stroke-accent'} transition-[stroke-dasharray] duration-700 ease-out motion-reduce:transition-none`} />
    </svg>
    {tick !== null && <span className="absolute h-3.5 w-0.5 rounded-full bg-text-primary/70" style={{
      left: centre + Math.cos(tick) * RADIUS - 1, top: centre + Math.sin(tick) * RADIUS - 7, transform: `rotate(${passMark! * 360}deg)`,
    }} />}
    <span className="absolute font-serif text-[1.9rem] tabular-nums leading-none text-text-primary">{label}</span>
  </div>;
}
