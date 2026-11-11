import type * as Wire from '@/lib/bindings';

import type { KindCopy } from '../../session/kind';

const INSTRUCTIONS: Record<Wire.Stage, string> = {
  understand: 'Read the statement and ask about anything it leaves open.',
  examples: 'Write examples with the answers you expect, before any code.',
  plan: 'Say how you will solve it and what that costs.',
  make: 'Write the function.',
  check: 'Run your examples, then submit to run every case.',
  review: 'Explain the cost and what you would change.',
  reproduce: 'Make the bug happen with an input you choose.',
  locate: 'Find the line where the result goes wrong.',
  fix: 'Change the code until the failing inputs pass.',
  verify: 'Submit to run every case and confirm nothing else broke.',
};

const RUNGS: Record<Wire.Rung, { name: string; ask: string }> = {
  nudge: { name: 'Nudge', ask: 'Get a nudge' },
  idea: { name: 'Idea', ask: 'Get the idea' },
  plan: { name: 'Plan', ask: 'Get a plan' },
  point: { name: 'Point at the line', ask: 'Point at the line' },
};

export const programCopy: KindCopy = {
  taskPanel: 'Problem',
  workPanel: 'Code',
  readingWork: 'Reading your code…',
  stageInstruction: (stage) => INSTRUCTIONS[stage],
  rung: (rung) => RUNGS[rung],
};
